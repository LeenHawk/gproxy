//! video conversion family: the OpenAI native video API (`/v1/videos`, the
//! Sora shape) against a Gemini upstream that generates video through Veo
//! `predictLongRunning`. Job state lives in `ProtocolState`; `adapt::video`
//! reserves it before the create call and never repeats an uncertain create.
//! It expires `adapt::VIDEO_STATE_TTL` (a week) after its latest write, so a
//! job unpolled that long is gone and its retrieve or download finds nothing.
//!
//! Deliberate boundaries:
//! * Create is JSON only. The multipart form with an `input_reference` part
//!   is refused; `input_reference` by `file_id` is read through `Resources`.
//! * Retrieve and content download find the job by the client id in the path;
//!   the id is the one core minted at create.
//! * Content download reads inline Veo bytes. A Veo result delivered as a
//!   private URI needs a URL read through the provider, which `Resources`
//!   refuses today, so such a job's content fails with `Unsupported`.
//! * Gemini has no list or delete for long-running operations; those
//!   operations are refused.

use super::{Call, Converted};
use crate::{ExecutionTarget, ResourceScope, Resources, StateScope};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{
        JsonInvocation,
        video::{self as adapt, VideoBinding, VideoJobState, VideoLimits, VideoProgress},
    },
    capability::{StateStore, Upstream},
    codec::{CodecError, CodecErrorKind, CodecLimits, decode_json, encode_json},
    transform::{
        TransformError, TransformErrorKind,
        video::{NativePendingStatus, NativeVideoDefaults, NativeVideoResponseFacts},
    },
    wire::{gemini::video as g, openai::video as o},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, Method, StatusCode};

/// The Gemini API prefix every Veo path hangs off.
const OPERATION_PREFIX: &str = "/v1beta/";
const MAX_RESOURCE_FACTS: usize = 8;
/// Sora's documented defaults when the client omits duration or size; Veo's
/// exact 720p mapping supports both 720p orientations.
const DEFAULTS: NativeVideoDefaults = NativeVideoDefaults {
    seconds: o::NativeVideoSeconds::Four,
    size: o::NativeVideoSize::Portrait720,
};

fn codec(error: CodecError, context: &'static str) -> TransformError {
    let kind = match error.kind() {
        CodecErrorKind::Limit => TransformErrorKind::Limit,
        CodecErrorKind::Transport => TransformErrorKind::Host,
        _ => TransformErrorKind::InvalidInput,
    };
    TransformError::with_source(kind, context, error.to_string(), error)
}

fn json_response<T: serde::Serialize>(
    status: StatusCode,
    value: &T,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    let body = encode_json(value, limits).map_err(|e| codec(e, "client.response"))?;
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(Converted::Success(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body),
    }))
}

fn template(method: Method, path: String) -> WireRequest<()> {
    WireRequest {
        method,
        path,
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

/// The client id named by `/v1/videos/{id}` or `/v1/videos/{id}/content`.
fn client_id<C>(call: &Call<'_, C>, content: bool) -> Result<String, TransformError> {
    let mut path = call.request.path;
    if content {
        path = path.strip_suffix("/content").ok_or_else(|| {
            TransformError::shape("video.path", "content download path must end in /content")
        })?;
    }
    let raw = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let id = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|e| TransformError::shape("video.path", e.to_string()))?
        .into_owned();
    if id.is_empty() || id == "videos" {
        return Err(TransformError::shape(
            "video.path",
            "video id missing from path",
        ));
    }
    Ok(id)
}

fn origin_of(base_url: Option<&str>) -> Result<String, TransformError> {
    let base = base_url.ok_or_else(|| TransformError::missing_metadata("provider.base_url"))?;
    let uri: http::Uri = base.parse().map_err(|e: http::uri::InvalidUri| {
        TransformError::shape("provider.base_url", e.to_string())
    })?;
    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => Ok(format!("{scheme}://{authority}")),
        _ => Err(TransformError::shape(
            "provider.base_url",
            "absolute base URL required",
        )),
    }
}

/// Job state is keyed by the client id alone, so any later request in the
/// same scope and provider finds it without a conversation.
fn job_scope<C>(call: &Call<'_, C>) -> StateScope {
    StateScope {
        scope: call.state_scope.scope.clone(),
        provider_id: call.provider_id.to_owned(),
        conversation: None,
    }
}

/// Mirrors `VideoBinding::key`, which the adapter keeps private.
fn job_key(client_id: &str) -> String {
    format!("video/job/{}:{}", client_id.len(), client_id)
}

async fn load_job<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    scope: &StateScope,
    client_id: &str,
) -> Result<VideoJobState, TransformError> {
    let entry = call
        .state_store
        .get(scope, &job_key(client_id))
        .await?
        .ok_or_else(|| TransformError::missing_metadata("video.job_state"))?;
    decode_json(&entry.payload, call.limits)
        .map_err(|e| TransformError::invalid_result("video.state", e.to_string()))
}

fn resources<'a, C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'a, C>,
) -> (Resources<'a, C>, ResourceScope) {
    let target = &call.upstream.attempt().request.target;
    (
        Resources::new(call.core, call.upstream.limits(), call.limits),
        ResourceScope {
            scope: call.state_scope.scope.clone(),
            target: ExecutionTarget {
                requested_model: None,
                provider: target.provider.clone(),
                upstream_model: target.upstream_model.clone(),
                credentials: target.credentials.clone(),
            },
        },
    )
}

/// What the client sees as stable job facts. Veo reports none of Sora's
/// timestamps, size, duration or progress, so they come from the request and
/// the moment core observed each transition, then stay fixed in job state.
struct Facts {
    client_id: String,
    created_at: i64,
    model: String,
    seconds: o::NativeVideoSeconds,
    size: o::NativeVideoSize,
    prompt: String,
    previous_progress: i64,
    completed_at: Option<i64>,
    pending: NativePendingStatus,
    now: i64,
}

impl Facts {
    fn build(
        self,
        operation: &g::VideoOperation,
    ) -> Result<NativeVideoResponseFacts, TransformError> {
        let done = operation.done == Some(true);
        Ok(NativeVideoResponseFacts {
            operation_name: operation
                .name
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("video.operation.name"))?,
            client_id: self.client_id,
            created_at: self.created_at,
            model: self.model,
            seconds: self.seconds,
            size: self.size,
            progress: if done { 100 } else { self.previous_progress },
            pending_status: self.pending,
            completed_at: if done && operation.error.is_none() {
                self.completed_at.or(Some(self.now))
            } else {
                None
            },
            expires_at: None,
            prompt: Some(self.prompt),
            failure_code: None,
        })
    }
}

fn finish(
    result: JsonInvocation<gproxy_protocol::transform::Converted<o::NativeVideo>>,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    match result {
        JsonInvocation::Success(response) => {
            json_response(response.status, &response.body.value, limits)
        }
        JsonInvocation::Rejected(response) => Ok(Converted::Rejected(response)),
    }
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    if !matches!(call.client.dialect, Dialect::OpenAi | Dialect::OpenAiChat) {
        return Err(TransformError::unsupported(
            "video",
            format!("{:?} has no video API", call.client.dialect),
        ));
    }
    if call.target != Dialect::Gemini {
        return Err(TransformError::unsupported(
            "video",
            format!("{:?} has no video generation API", call.target),
        ));
    }
    match call.client.operation {
        Operation::CreateVideo => create(call).await,
        Operation::RetrieveVideo => retrieve(call).await,
        Operation::DownloadVideoContent => content(call).await,
        Operation::ListVideos | Operation::DeleteVideo => Err(TransformError::unsupported(
            "video",
            "Gemini long-running operations cannot be listed or deleted",
        )),
        other => Err(TransformError::unsupported(
            "video",
            format!("{other:?} is not a video operation"),
        )),
    }
}

fn limits<C>(call: &Call<'_, C>) -> VideoLimits {
    VideoLimits {
        codec: call.limits,
        max_resource_facts: MAX_RESOURCE_FACTS,
    }
}

async fn create<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let is_multipart = call
        .request
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim_start().starts_with("multipart/"));
    if is_multipart {
        return Err(TransformError::unsupported(
            "video.create",
            "multipart video creation with an uploaded input_reference is not converted; send JSON with a file_id",
        ));
    }
    let input: o::NativeCreateVideoRequestBody =
        decode_json(call.body(), call.limits).map_err(|e| codec(e, "client.body"))?;
    let attempt = call.upstream.attempt();
    let provider = &attempt.request.target.provider;
    let client_id = format!("video_{}", crate::ids::random_id());
    let binding = VideoBinding {
        client_id: client_id.clone(),
        origin: origin_of(provider.entity.base_url.as_deref())?,
        model: call.model()?.to_owned(),
        polling_url: format!("/v1/videos/{client_id}"),
        operation_prefix: OPERATION_PREFIX.into(),
    };
    let facts = Facts {
        client_id: client_id.clone(),
        created_at: call.now_ms.div_euclid(1000),
        model: input.model.clone().unwrap_or_else(|| binding.model.clone()),
        seconds: input.seconds.unwrap_or(DEFAULTS.seconds),
        size: input.size.unwrap_or(DEFAULTS.size),
        prompt: input.prompt.clone(),
        previous_progress: 0,
        completed_at: None,
        pending: NativePendingStatus::Queued,
        now: call.now_ms.div_euclid(1000),
    };
    let path = format!(
        "{OPERATION_PREFIX}models/{}:predictLongRunning",
        binding
            .model
            .strip_prefix("models/")
            .unwrap_or(&binding.model)
    );
    let (resources, resource_scope) = resources(call);
    let scope = job_scope(call);
    let key = OperationKey {
        operation: Operation::CreateVideo,
        dialect: call.target,
    };
    let mut progress = VideoProgress::default();
    let result = adapt::native_to_gemini_create_composed(
        call.upstream,
        &key,
        &resources,
        &resource_scope,
        call.state_store,
        &scope,
        template(Method::POST, path),
        input,
        Some(DEFAULTS),
        binding,
        |operation| facts.build(operation),
        &mut progress,
        limits(call),
    )
    .await?;
    finish(result, call.limits)
}

/// Facts for a later poll: everything stable comes from the saved state.
fn continued_facts(state: &VideoJobState, now: i64) -> Result<Facts, TransformError> {
    let saved = state
        .native_facts
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("video.native_facts"))?;
    Ok(Facts {
        client_id: saved.client_id.clone(),
        created_at: saved.created_at,
        model: saved.model.clone(),
        seconds: saved.seconds,
        size: saved.size,
        prompt: saved.prompt.clone().unwrap_or_default(),
        previous_progress: saved.progress,
        completed_at: saved.completed_at,
        pending: NativePendingStatus::InProgress,
        now,
    })
}

async fn retrieve<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let id = client_id(call, false)?;
    let scope = job_scope(call);
    let state = load_job(call, &scope, &id).await?;
    let facts = continued_facts(&state, call.now_ms.div_euclid(1000))?;
    let name = state
        .operation
        .as_ref()
        .and_then(|op| op.name.clone())
        .ok_or_else(|| TransformError::missing_metadata("video.operation.name"))?;
    let path = format!("{}{name}", state.binding.operation_prefix);
    let key = OperationKey {
        operation: Operation::RetrieveVideo,
        dialect: call.target,
    };
    let mut progress = VideoProgress::default();
    let result = adapt::native_to_gemini_query_composed(
        call.upstream,
        &key,
        call.state_store,
        &scope,
        template(Method::GET, path),
        state.binding.clone(),
        |operation| facts.build(operation),
        &mut progress,
        limits(call),
    )
    .await?;
    finish(result, call.limits)
}

async fn content<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let id = client_id(call, true)?;
    let scope = job_scope(call);
    let state = load_job(call, &scope, &id).await?;
    let (resources, resource_scope) = resources(call);
    let read = adapt::native_content(
        &resources,
        &resource_scope,
        call.state_store,
        &scope,
        state.binding,
        limits(call),
    )
    .await?;
    let mut headers = HeaderMap::new();
    if let Some(mime) = read.metadata.mime.as_deref()
        && let Ok(value) = HeaderValue::from_str(mime)
    {
        headers.insert(http::header::CONTENT_TYPE, value);
    }
    if let Some(length) = read.metadata.length {
        headers.insert(
            http::header::CONTENT_LENGTH,
            HeaderValue::from_str(&length.to_string()).expect("digits"),
        );
    }
    Ok(Converted::Success(WireResponse {
        status: StatusCode::OK,
        headers,
        body: read.body,
    }))
}
