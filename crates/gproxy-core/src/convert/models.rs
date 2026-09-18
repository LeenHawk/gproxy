//! Model directory conversion. Listing collects the upstream's complete
//! directory (bounded) and maps it with per-model facts; a single lookup
//! fetches the native object and maps it. Facts a target schema requires and
//! the source lacks (OpenAI owner/created, Gemini base model/version, Claude
//! capabilities) come from the provider's model rows: `provider_models.metadata`
//! `supplements.{openai|claude|gemini}` keyed by the upstream model name. A
//! model without the needed supplement fails the conversion rather than being
//! invented.

use super::{Call, Converted};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{
        JsonInvocation, invoke_empty,
        models::{
            self as adapt, ModelDirectory, ModelListError, ModelListFailure, ModelListLimits,
        },
    },
    codec::{CodecLimits, encode_json},
    transform::{
        Converted as Mapped, TransformError, TransformErrorKind,
        models::{
            self as transform, ClaudeModelSupplement, GeminiModelSupplement, OpenAiModelSupplement,
        },
    },
    wire::{claude::models as c, gemini::models as g, openai::models as o},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, Method};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

/// Pages fetched for one directory listing.
const MAX_LIST_CALLS: usize = 16;
/// Models retained across the pages of one listing.
const MAX_LIST_MODELS: usize = 4096;

fn list_limits(codec: CodecLimits) -> ModelListLimits {
    ModelListLimits {
        codec,
        max_calls: MAX_LIST_CALLS,
        max_models: MAX_LIST_MODELS,
        max_declared_bytes: codec.max_body_bytes.saturating_mul(MAX_LIST_CALLS as u64),
    }
}

/// Supplements of one family from the provider's model rows, keyed by the
/// upstream name. Gemini directories name models `models/{id}`, so a Gemini
/// upstream's rows are reachable under both spellings.
fn supplements<T: DeserializeOwned + Clone, C>(
    call: &Call<'_, C>,
    family: &str,
) -> BTreeMap<String, T> {
    let mut map = BTreeMap::new();
    for model in &call.upstream.attempt().request.target.provider.models {
        let Some(value) = model
            .metadata
            .get("supplements")
            .and_then(|s| s.get(family))
        else {
            continue;
        };
        let Ok(supplement) = serde_json::from_value::<T>(value.clone()) else {
            continue;
        };
        if call.target == Dialect::Gemini && !model.upstream_name.starts_with("models/") {
            map.insert(
                format!("models/{}", model.upstream_name),
                supplement.clone(),
            );
        }
        map.insert(model.upstream_name.clone(), supplement);
    }
    map
}

fn encode<T: Serialize>(
    value: &T,
    status: http::StatusCode,
    headers: HeaderMap,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    let body = encode_json(value, limits).map_err(|e| {
        TransformError::with_source(
            TransformErrorKind::Limit,
            "models.response",
            e.to_string(),
            e,
        )
    })?;
    Ok(Converted::Success(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(body),
    }))
}

fn directory<T: Serialize>(
    result: Result<ModelDirectory<Mapped<T>>, ModelListError>,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    match result {
        Ok(directory) => encode(
            &directory.value.value,
            http::StatusCode::OK,
            HeaderMap::new(),
            limits,
        ),
        Err(ModelListError {
            failure: ModelListFailure::Rejected(response),
            ..
        }) => Ok(Converted::Rejected(*response)),
        Err(ModelListError {
            failure: ModelListFailure::Transform(error),
            ..
        }) => Err(error),
    }
}

fn template<C>(call: &Call<'_, C>, path: String) -> WireRequest<()> {
    WireRequest {
        method: Method::GET,
        path,
        query: None,
        headers: call.request.headers.clone(),
        body: (),
    }
}

async fn list<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    if call.request.query.as_deref().is_some_and(|q| !q.is_empty()) {
        return Err(TransformError::unsupported(
            "models.query",
            "client pagination is not converted; the complete directory is returned",
        ));
    }
    let key = OperationKey {
        operation: Operation::ListModels,
        dialect: call.target,
    };
    let request = template(
        call,
        super::endpoints::list_models_path(call.target)?.to_owned(),
    );
    let limits = list_limits(call.limits);
    let upstream = call.upstream;
    match (call.client.dialect, call.target) {
        (Dialect::OpenAi | Dialect::OpenAiChat, Dialect::Claude) => directory(
            adapt::claude_to_openai_list(
                upstream,
                &key,
                request,
                c::ListModelsQuery::builder().build(),
                &supplements::<OpenAiModelSupplement, C>(call, "openai"),
                limits,
            )
            .await,
            call.limits,
        ),
        (Dialect::OpenAi | Dialect::OpenAiChat, Dialect::Gemini) => directory(
            adapt::gemini_to_openai_list(
                upstream,
                &key,
                request,
                g::ListModelsQuery::builder().build(),
                &supplements::<OpenAiModelSupplement, C>(call, "openai"),
                limits,
            )
            .await,
            call.limits,
        ),
        (Dialect::Claude, Dialect::OpenAi | Dialect::OpenAiChat) => directory(
            adapt::openai_to_claude_list(
                upstream,
                &key,
                request,
                &supplements::<ClaudeModelSupplement, C>(call, "claude"),
                None,
                limits,
            )
            .await,
            call.limits,
        ),
        (Dialect::Claude, Dialect::Gemini) => directory(
            adapt::gemini_to_claude_list(
                upstream,
                &key,
                request,
                g::ListModelsQuery::builder().build(),
                &supplements::<ClaudeModelSupplement, C>(call, "claude"),
                None,
                limits,
            )
            .await,
            call.limits,
        ),
        (Dialect::Gemini, Dialect::OpenAi | Dialect::OpenAiChat) => directory(
            adapt::openai_to_gemini_list(
                upstream,
                &key,
                request,
                &supplements::<GeminiModelSupplement, C>(call, "gemini"),
                limits,
            )
            .await,
            call.limits,
        ),
        (Dialect::Gemini, Dialect::Claude) => directory(
            adapt::claude_to_gemini_list(
                upstream,
                &key,
                request,
                c::ListModelsQuery::builder().build(),
                &supplements::<GeminiModelSupplement, C>(call, "gemini"),
                limits,
            )
            .await,
            call.limits,
        ),
        (client, target) => Err(TransformError::unsupported(
            "models.list",
            format!("no conversion from {client:?} to {target:?}"),
        )),
    }
}

/// The bare model id from the client's own path: the segment after the last
/// `/models/`, percent-decoded, without a Gemini `models/` resource prefix.
fn requested_id(path: &str) -> Result<String, TransformError> {
    let (_, tail) = path
        .trim_end_matches('/')
        .rsplit_once("/models/")
        .ok_or_else(|| TransformError::shape("request.path", "expected a `/models/{id}` path"))?;
    let id = percent_encoding::percent_decode_str(tail)
        .decode_utf8()
        .map_err(|_| TransformError::shape("request.path", "model id is not UTF-8"))?;
    let id = id.strip_prefix("models/").unwrap_or(&id);
    if id.is_empty() {
        return Err(TransformError::shape("request.path", "empty model id"));
    }
    Ok(id.to_owned())
}

fn supplement<'m, T>(
    map: &'m BTreeMap<String, T>,
    family: &str,
    id: &str,
) -> Result<&'m T, TransformError> {
    map.get(id)
        .ok_or_else(|| TransformError::missing_metadata(format!("supplements.{family} for `{id}`")))
}

async fn get<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let id = requested_id(&call.request.path)?;
    let key = OperationKey {
        operation: Operation::GetModel,
        dialect: call.target,
    };
    let request = template(call, super::endpoints::get_model_path(call.target, &id)?);
    let upstream = call.upstream;
    let limits = call.limits;
    macro_rules! fetch {
        ($native:ty) => {{
            match invoke_empty::<_, $native>(upstream, &key, request, limits).await? {
                JsonInvocation::Rejected(response) => return Ok(Converted::Rejected(response)),
                JsonInvocation::Success(response) => response,
            }
        }};
    }
    match (call.client.dialect, call.target) {
        (Dialect::OpenAi | Dialect::OpenAiChat, Dialect::Claude) => {
            let native = fetch!(c::ModelInfo);
            let map = supplements::<OpenAiModelSupplement, C>(call, "openai");
            let facts = supplement(&map, "openai", &native.body.id)?;
            let mapped = transform::claude_to_openai(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (Dialect::OpenAi | Dialect::OpenAiChat, Dialect::Gemini) => {
            let native = fetch!(g::Model);
            let map = supplements::<OpenAiModelSupplement, C>(call, "openai");
            let facts = supplement(&map, "openai", &native.body.name)?;
            let mapped = transform::gemini_to_openai(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (Dialect::Claude, Dialect::OpenAi | Dialect::OpenAiChat) => {
            let native = fetch!(o::Model);
            let map = supplements::<ClaudeModelSupplement, C>(call, "claude");
            let facts = supplement(&map, "claude", &native.body.id)?;
            let mapped = transform::openai_to_claude(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (Dialect::Claude, Dialect::Gemini) => {
            let native = fetch!(g::Model);
            let map = supplements::<ClaudeModelSupplement, C>(call, "claude");
            let facts = supplement(&map, "claude", &native.body.name)?;
            let mapped = transform::gemini_to_claude(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (Dialect::Gemini, Dialect::OpenAi | Dialect::OpenAiChat) => {
            let native = fetch!(o::Model);
            let map = supplements::<GeminiModelSupplement, C>(call, "gemini");
            let facts = supplement(&map, "gemini", &native.body.id)?;
            let mapped = transform::openai_to_gemini(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (Dialect::Gemini, Dialect::Claude) => {
            let native = fetch!(c::ModelInfo);
            let map = supplements::<GeminiModelSupplement, C>(call, "gemini");
            let facts = supplement(&map, "gemini", &native.body.id)?;
            let mapped = transform::claude_to_gemini(native.body, facts)?;
            encode(&mapped.value, native.status, native.headers, limits)
        }
        (client, target) => Err(TransformError::unsupported(
            "models.get",
            format!("no conversion from {client:?} to {target:?}"),
        )),
    }
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    match call.client.operation {
        Operation::ListModels => list(call).await,
        Operation::GetModel => get(call).await,
        other => Err(TransformError::unsupported(
            "models",
            format!("{other:?} is not a model directory operation"),
        )),
    }
}
