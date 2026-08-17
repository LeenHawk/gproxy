//! Locally-served operations (§6.3): model-list/get shaping plus the no-upstream
//! serving of `Local`-plan candidates (count_tokens via [`crate::tokenize`],
//! models from the snapshot's exposed rows).
//! Minimal-field JSON on purpose — list shape per the protocol modules
//! (`openai::models`, `claude::models`, and `gemini::models`), optional fields
//! omitted or zero-valued.

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::app::models_index::ExposedModel;
use crate::app::snapshot::ControlPlaneSnapshot;
use crate::channel::disposition::Disposition;
use crate::pipeline::classify;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::model_limits::{ModelLimits, ModelThinking};
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::protocol::{Operation, Provider};

/// One gateway-visible model for rendering.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: Option<String>,
    pub limits: ModelLimits,
    pub thinking: ModelThinking,
}

/// Serve a `Local`-plan candidate without an upstream call (§6.3). `None` =
/// the op has no local implementation (caller maps to `LocalUnimplemented`).
pub fn serve_local(
    state: &AppState,
    cp: &ControlPlaneSnapshot,
    ctx: &RequestCtx,
    cand: &Candidate,
) -> Option<ExecOutcome> {
    let op = ctx.op.expect("classified before failover");
    let family = op.provider_family();
    match op.operation() {
        Operation::CountTokens => Some(local_count(state, ctx, cand, family)),
        Operation::GetModel => {
            let id = classify::path_model_id(&ctx.path);
            let entries = exposed_entries(cp, cand.provider.id);
            let found = id
                .as_deref()
                .and_then(|id| entries.iter().find(|e| e.id == id));
            Some(match found {
                Some(e) => json_outcome(StatusCode::OK, render_model(family, e)),
                None => json_outcome(
                    StatusCode::NOT_FOUND,
                    to_bytes(&json!({ "error": { "message": "model not found" } })),
                ),
            })
        }
        _ => None,
    }
}

/// §6.3 local count: tokenize the inbound body and answer in the INBOUND
/// family's wire shape. Never fails (tokenize::count floors to an estimate).
fn local_count(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    family: Provider,
) -> ExecOutcome {
    // pre-variant-strip name is fine for tokenizer selection; `body_model` is
    // classify's single peek — no body re-parse here
    let model = ctx
        .body_model
        .clone()
        .or_else(|| classify::path_model_id(&ctx.path))
        .unwrap_or_else(|| cand.upstream_model_id.clone());
    let map = cand.provider.settings_json.get("tokenizer_map");
    #[cfg(feature = "count-local")]
    let n = crate::tokenize::count(&model, &ctx.body, map, &state.tokenizers);
    #[cfg(not(feature = "count-local"))]
    let n = {
        let _ = state;
        crate::tokenize::count(&model, &ctx.body, map, ())
    };
    let body = match family {
        Provider::Claude => json!({ "input_tokens": n }),
        Provider::Gemini => json!({ "totalTokens": n }),
        // minimal `/v1/responses/input_tokens` response shape
        Provider::OpenAi => json!({ "object": "response.input_tokens", "input_tokens": n }),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    json_outcome(StatusCode::OK, to_bytes(&body))
}

/// Exposed-model rows as render entries (empty when the provider has none).
fn exposed_entries(cp: &ControlPlaneSnapshot, provider_id: i64) -> Vec<ModelEntry> {
    cp.exposed_models_by_provider
        .get(&provider_id)
        .map(|m| entries_from(m))
        .unwrap_or_default()
}

/// [`ExposedModel`] rows → render entries.
pub fn entries_from(models: &[ExposedModel]) -> Vec<ModelEntry> {
    models
        .iter()
        .map(|m| ModelEntry {
            id: m.full_id.clone(),
            display_name: m.display_name.clone(),
            limits: ModelLimits::new(m.context_window, m.max_output_tokens),
            thinking: ModelThinking::new(
                m.thinking_supported,
                m.thinking_adaptive_supported,
                m.thinking_enabled_supported,
            ),
        })
        .collect()
}

/// Buffered-JSON outcome shared by the local-serving paths.
pub fn json_outcome(status: StatusCode, body: Bytes) -> ExecOutcome {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    let disposition = if status.is_success() {
        Disposition::Success
    } else {
        Disposition::Permanent
    };
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(body),
        disposition,
    }
}

/// Serialize a model list in the inbound wire kind's list shape.
pub fn render_model_list(family: Provider, entries: &[ModelEntry]) -> Bytes {
    let items: Vec<Value> = entries.iter().map(|e| entry_value(family, e)).collect();
    let list = match family {
        Provider::OpenAi => json!({ "object": "list", "data": items }),
        Provider::Claude => json!({
            "data": items,
            "first_id": entries.first().map(|e| e.id.as_str()),
            "last_id": entries.last().map(|e| e.id.as_str()),
            "has_more": false,
        }),
        Provider::Gemini => json!({ "models": items }),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    to_bytes(&list)
}

/// Render one model (GetModel) in the family's single-model shape.
pub fn render_model(family: Provider, entry: &ModelEntry) -> Bytes {
    to_bytes(&entry_value(family, entry))
}

/// One model object in the family's entry shape.
fn entry_value(family: Provider, e: &ModelEntry) -> Value {
    match family {
        Provider::OpenAi => {
            let mut model = json!({
                "id": e.id,
                "object": "model",
                "created": 0,
                "owned_by": "GPROXY",
            });
            if let Some(limit) = e.limits.context_window {
                model["context_length"] = json!(limit);
                model["context_window"] = json!(limit);
            }
            if let Some(limit) = e.limits.max_output_tokens {
                model["max_completion_tokens"] = json!(limit);
            }
            if e.thinking.supported == Some(true) {
                model["supported_parameters"] = json!(["reasoning"]);
            }
            model
        }
        Provider::Claude => {
            let mut model = json!({
                "id": e.id,
                "type": "model",
                "display_name": e.display_name.as_deref().unwrap_or(&e.id),
                "created_at": "1970-01-01T00:00:00Z",
            });
            if let Some(limit) = e.limits.context_window {
                model["max_input_tokens"] = json!(limit);
            }
            if let Some(limit) = e.limits.max_output_tokens {
                model["max_tokens"] = json!(limit);
            }
            if e.thinking.supported.is_some()
                || e.thinking.adaptive_supported.is_some()
                || e.thinking.enabled_supported.is_some()
            {
                let mut thinking = json!({});
                if let Some(supported) = e.thinking.supported {
                    thinking["supported"] = json!(supported);
                }
                let mut types = json!({});
                if let Some(supported) = e.thinking.adaptive_supported {
                    types["adaptive"] = json!({ "supported": supported });
                }
                if let Some(supported) = e.thinking.enabled_supported {
                    types["enabled"] = json!({ "supported": supported });
                }
                thinking["types"] = types;
                model["capabilities"] = json!({ "thinking": thinking });
            }
            model
        }
        Provider::Gemini => {
            let mut model = json!({ "name": wire_id(family, &e.id) });
            if let Some(d) = &e.display_name {
                model["displayName"] = json!(d);
            }
            if let Some(limit) = e.limits.context_window {
                model["inputTokenLimit"] = json!(limit);
            }
            if let Some(limit) = e.limits.max_output_tokens {
                model["outputTokenLimit"] = json!(limit);
            }
            if let Some(supported) = e.thinking.supported {
                model["thinking"] = json!(supported);
            }
            model
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

/// The id as it appears on the wire (gemini prefixes `models/`).
fn wire_id(family: Provider, id: &str) -> String {
    match family {
        Provider::Gemini => format!("models/{id}"),
        _ => id.to_owned(),
    }
}

fn to_bytes(v: &Value) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("json! value serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::model_limits::{ModelLimits, ModelThinking};

    fn entry() -> ModelEntry {
        ModelEntry {
            id: "test-model".into(),
            display_name: Some("Test Model".into()),
            limits: ModelLimits::new(Some(128_000), Some(8_000)),
            thinking: ModelThinking::new(Some(true), Some(true), Some(false)),
        }
    }

    #[test]
    fn model_limits_render_in_each_native_shape() {
        let openai: Value =
            serde_json::from_slice(&render_model(Provider::OpenAi, &entry())).unwrap();
        assert_eq!(openai["context_length"], 128_000);
        assert_eq!(openai["context_window"], 128_000);
        assert_eq!(openai["max_completion_tokens"], 8_000);
        assert!(openai.get("max_input_tokens").is_none());
        assert_eq!(openai["supported_parameters"], json!(["reasoning"]));

        let mut unsupported = entry();
        unsupported.thinking.supported = Some(false);
        let openai_unsupported: Value =
            serde_json::from_slice(&render_model(Provider::OpenAi, &unsupported)).unwrap();
        assert!(openai_unsupported.get("supported_parameters").is_none());

        let claude: Value =
            serde_json::from_slice(&render_model(Provider::Claude, &entry())).unwrap();
        assert_eq!(claude["max_input_tokens"], 128_000);
        assert_eq!(claude["max_tokens"], 8_000);
        assert!(claude.get("context_window").is_none());
        assert_eq!(claude["capabilities"]["thinking"]["supported"], true);
        assert_eq!(
            claude["capabilities"]["thinking"]["types"]["adaptive"]["supported"],
            true
        );
        assert_eq!(
            claude["capabilities"]["thinking"]["types"]["enabled"]["supported"],
            false
        );

        let gemini: Value =
            serde_json::from_slice(&render_model(Provider::Gemini, &entry())).unwrap();
        assert_eq!(gemini["inputTokenLimit"], 128_000);
        assert_eq!(gemini["outputTokenLimit"], 8_000);
        assert!(gemini.get("context_window").is_none());
        assert_eq!(gemini["thinking"], true);
    }
}
