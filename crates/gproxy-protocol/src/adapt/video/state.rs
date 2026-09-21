use super::*;
use crate::{
    capability::{CasResult, ResourceReference, StateStore, StateWrite, Version},
    transform::TransformErrorKind,
};
use serde::{Deserialize, Serialize};

/// Stable host facts. `origin` must identify the selected target in the host's
/// state scope. The host supplies its API prefix; it is never inferred from IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoBinding {
    pub client_id: String,
    pub origin: String,
    pub model: String,
    pub polling_url: String,
    pub operation_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoOriginalRequest {
    OpenRouter(o::CreateVideoRequestBody),
    Native(o::NativeCreateVideoRequestBody),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJobState {
    pub schema: u32,
    pub binding: VideoBinding,
    pub original: VideoOriginalRequest,
    pub native_facts: Option<video::NativeVideoResponseFacts>,
    pub effective: g::PredictLongRunningRequestBody,
    /// None means reserved with an uncertain creation outcome. It is never
    /// permission to repeat the create request.
    pub operation: Option<g::VideoOperation>,
    pub published_urls: BTreeMap<String, String>,
}

/// Caller-owned progress survives dropped futures and post-create failures.
/// The native response is retained before binding/publication work begins.
#[derive(Debug, Default)]
pub struct VideoProgress {
    pub(super) binding: Option<VideoBinding>,
    pub reserved: bool,
    pub send_started: bool,
    pub binding_saved: bool,
    pub raw_response: Option<crate::WireResponse<bytes::Bytes>>,
    pub native_response: Option<crate::WireResponse<g::VideoOperation>>,
    pub publication_ids: Vec<String>,
    pub publications: Vec<ResourceReference>,
}

impl VideoBinding {
    pub(super) fn create_path(&self) -> Result<String, TransformError> {
        let model = self.model.strip_prefix("models/").unwrap_or(&self.model);
        if model.is_empty()
            || !model
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~'))
            || matches!(model, "." | "..")
        {
            return Err(TransformError::shape(
                "video.model",
                "Developer API model ID or models/{id} required",
            ));
        }
        Ok(format!(
            "{}models/{model}:predictLongRunning",
            self.operation_prefix
        ))
    }
    pub(super) fn key(&self) -> String {
        format!("video/job/{}:{}", self.client_id.len(), self.client_id)
    }
    pub(super) fn query_path(&self, name: &str) -> Result<String, TransformError> {
        let parts: Vec<_> = name.split('/').collect();
        if parts.len() < 2
            || parts[parts.len() - 2] != "operations"
            || parts
                .iter()
                .any(|s| s.is_empty() || matches!(*s, "." | ".."))
            || name
                .chars()
                .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '~')))
        {
            return Err(TransformError::shape(
                "video.operation.name",
                "safe operations resource name required",
            ));
        }
        Ok(format!("{}{name}", self.operation_prefix))
    }
}

pub(super) fn public_url(value: &str) -> Result<(), TransformError> {
    let uri: http::Uri = value
        .parse()
        .map_err(|e: http::uri::InvalidUri| TransformError::shape("video.url", e.to_string()))?;
    if !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
    {
        return Err(TransformError::shape(
            "video.url",
            "absolute HTTP(S) URL without credentials required",
        ));
    }
    Ok(())
}

pub(super) async fn save<S: StateStore>(
    store: &S,
    scope: &S::Scope,
    state: &VideoJobState,
    expected: Option<Version>,
    limits: VideoLimits,
) -> Result<Version, TransformError> {
    save_payload(store, scope, &state.binding.key(), state, expected, limits).await
}

pub(super) async fn save_payload<S: StateStore, T: Serialize>(
    store: &S,
    scope: &S::Scope,
    key: &str,
    state: &T,
    expected: Option<Version>,
    limits: VideoLimits,
) -> Result<Version, TransformError> {
    let mut codec = limits.codec;
    codec.max_body_bytes = codec.max_body_bytes.min(store.limits().write_bytes);
    codec.max_buffer_bytes = codec.max_buffer_bytes.min(codec.max_body_bytes);
    codec.max_value_bytes = codec.max_value_bytes.min(codec.max_body_bytes);
    let payload = crate::codec::encode_json(state, codec).map_err(|e| {
        TransformError::new(TransformErrorKind::Limit, "video.state", e.to_string())
    })?;
    match store
        .compare_exchange(
            scope,
            key,
            expected,
            Some(StateWrite {
                payload,
                expires_at: None,
            }),
        )
        .await?
    {
        CasResult::Applied(Some(version)) => Ok(version),
        CasResult::Applied(None) => Err(TransformError::invalid_result(
            "video.state",
            "CAS replacement did not return a version",
        )),
        CasResult::Conflict => Err(TransformError::new(
            TransformErrorKind::Conflict,
            "video.state",
            "job reservation or update conflicted; creation must not be retried",
        )),
    }
}

pub(super) async fn load<S: StateStore>(
    store: &S,
    scope: &S::Scope,
    binding: &VideoBinding,
    limits: VideoLimits,
) -> Result<(VideoJobState, Version), TransformError> {
    let entry = store
        .get(scope, &binding.key())
        .await?
        .ok_or_else(|| TransformError::missing_metadata("video.job_state"))?;
    if entry.payload.len() as u64 > store.limits().read_bytes {
        return Err(super::resources::limit("video.state"));
    }
    let mut state: VideoJobState = crate::codec::decode_json(&entry.payload, limits.codec)
        .map_err(|e| TransformError::invalid_result("video.state", e.to_string()))?;
    if state.schema != 1 || &state.binding != binding {
        return Err(TransformError::shape(
            "video.state.binding",
            "schema/client/origin/model/polling/API binding differs",
        ));
    }
    state.original = match state.original {
        VideoOriginalRequest::OpenRouter(v) => VideoOriginalRequest::OpenRouter(v.into_declared()),
        VideoOriginalRequest::Native(v) => VideoOriginalRequest::Native(v.into_declared()),
    };
    state.effective = state.effective.into_declared();
    state.operation = state.operation.map(DeclaredFields::into_declared);
    Ok((state, entry.version))
}
