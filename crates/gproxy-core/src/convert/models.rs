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

fn list_limits(codec: CodecLimits) -> ModelListLimits {
    ModelListLimits {
        codec,

        // Preserve the aggregate byte budget independently of page count.
        max_declared_bytes: codec.max_body_bytes.saturating_mul(16),
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
        let value = model
            .metadata
            .get("supplements")
            .and_then(|s| s.get(family))
            .cloned()
            .unwrap_or_else(|| {
                supplement_metadata(
                    &model.metadata,
                    family,
                    &call.upstream.attempt().request.target.provider.entity.name,
                )
            });
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
    if call.request.query.is_some_and(|q| !q.is_empty()) {
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
    let id = requested_id(call.request.path)?;
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

fn supplement_metadata(
    metadata: &serde_json::Value,
    family: &str,
    provider: &str,
) -> serde_json::Value {
    use serde_json::json;
    match family {
        "openai" => {
            json!({"owned_by": metadata.get("owned_by").and_then(serde_json::Value::as_str).unwrap_or(provider), "created": metadata.get("created")})
        }
        "gemini" => {
            json!({"base_model_id": metadata.get("base_model_id").and_then(serde_json::Value::as_str).unwrap_or(""), "version": metadata.get("version").and_then(serde_json::Value::as_str).unwrap_or("")})
        }
        "claude" => {
            let flag = |key: &str| {
                metadata
                    .get(key)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            };
            let modality = |key: &str| {
                metadata
                    .get("input_modalities")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(key)))
            };
            json!({"display_name": metadata.get("display_name"), "created_at": metadata.get("created_at"), "allowed_fallback_models": [], "max_input_tokens": metadata.get("context_window"), "max_tokens": metadata.get("max_output_tokens"), "capabilities": {
                "batch": flag("batch_supported"), "citations": flag("citations_supported"), "code_execution": flag("code_execution_supported"), "image_input": modality("image"), "pdf_input": flag("pdf_input_supported"), "structured_outputs": flag("structured_outputs_supported"),
                "context_management": {"supported": flag("context_management_supported"), "clear_thinking_20251015": false, "clear_tool_uses_20250919": false, "compact_20260112": false},
                "effort": {"supported": false, "low": false, "medium": false, "high": false, "xhigh": false, "max": false},
                "thinking": {"supported": flag("thinking_supported"), "adaptive": flag("thinking_adaptive_supported"), "enabled": flag("thinking_enabled_supported")}
            }})
        }
        _ => json!({}),
    }
}

pub(super) fn local_metadata(
    metadata: &serde_json::Value,
    dialect: Dialect,
    provider: &str,
) -> serde_json::Value {
    use serde_json::{Value, json};
    let mut value = metadata
        .get("supplements")
        .and_then(|s| s.get(dialect.id()))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let object = value.as_object_mut().expect("object");
    let fields: &[(&str, &str)] = match dialect {
        Dialect::Claude => &[
            ("display_name", "display_name"),
            ("context_window", "max_input_tokens"),
            ("max_output_tokens", "max_tokens"),
        ],
        Dialect::Gemini => &[
            ("display_name", "displayName"),
            ("description", "description"),
            ("context_window", "inputTokenLimit"),
            ("max_output_tokens", "outputTokenLimit"),
            ("generation_methods", "supportedGenerationMethods"),
            ("thinking_supported", "thinking"),
        ],
        _ => &[
            ("display_name", "display_name"),
            ("description", "description"),
            ("context_window", "context_window"),
            ("max_context_window", "max_context_window"),
            ("max_output_tokens", "max_output_tokens"),
            ("thinking_supported", "thinking_supported"),
            ("input_modalities", "input_modalities"),
            ("output_modalities", "output_modalities"),
            ("supported_parameters", "supported_parameters"),
            ("reasoning_levels", "reasoning_levels"),
            ("default_reasoning_level", "default_reasoning_level"),
            ("service_tiers", "service_tiers"),
            ("default_service_tier", "default_service_tier"),
        ],
    };
    for (from, to) in fields {
        if let Some(v) = metadata.get(from).filter(|v| !v.is_null()) {
            object.insert((*to).into(), v.clone());
        }
    }
    if dialect == Dialect::OpenAi {
        object
            .entry("owned_by")
            .or_insert(Value::String(provider.into()));
    }
    value
}
