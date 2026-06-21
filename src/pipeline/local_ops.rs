//! Locally-served operations (§6.3): model-list/get shaping (gateway-side
//! lists in each family's wire shape, manual rows merged into upstream lists)
//! plus the no-upstream serving of `Local`-plan candidates (count_tokens via
//! [`crate::tokenize`], models from the snapshot's exposed rows).
//! Minimal-field JSON on purpose — list shape per the protocol modules
//! ([`openai::models`](crate::protocol::openai::models),
//! [`claude::models`](crate::protocol::claude::models),
//! [`gemini::models`](crate::protocol::gemini::models)), optional fields
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
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::protocol::{Operation, Provider};

/// One gateway-visible model for rendering.
pub struct ModelEntry {
    pub id: String,
    pub display_name: Option<String>,
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
    match op.operation {
        Operation::CountTokens => Some(local_count(state, ctx, cand, family)),
        Operation::ListModels => {
            let entries = exposed_entries(cp, cand.provider.id);
            Some(json_outcome(
                StatusCode::OK,
                render_model_list(family, &entries),
            ))
        }
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
    // pre-variant-strip name is fine for tokenizer selection
    let model = classify::peek_model(&ctx.body)
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
    };
    to_bytes(&list)
}

/// Render one model (GetModel) in the family's single-model shape.
pub fn render_model(family: Provider, entry: &ModelEntry) -> Bytes {
    to_bytes(&entry_value(family, entry))
}

/// Merge `additions` into an upstream list body of `family` shape: dedup by id,
/// append the rest. Unparsable body → warn + return the original untouched.
pub fn merge_into_list(family: Provider, body: Bytes, additions: &[ModelEntry]) -> Bytes {
    let Ok(mut v) = serde_json::from_slice::<Value>(&body) else {
        tracing::warn!("model-list merge: upstream body is not JSON; left untouched");
        return body;
    };
    let key = match family {
        Provider::OpenAi | Provider::Claude => "data",
        Provider::Gemini => "models",
    };
    let Some(arr) = v.get_mut(key).and_then(Value::as_array_mut) else {
        tracing::warn!(key, "model-list merge: list array missing; left untouched");
        return body;
    };
    let existing: Vec<String> = arr.iter().filter_map(|m| entry_id(family, m)).collect();
    for add in additions {
        let wire_id = wire_id(family, &add.id);
        if !existing.contains(&wire_id) {
            arr.push(entry_value(family, add));
        }
    }
    to_bytes(&v)
}

/// One model object in the family's entry shape.
fn entry_value(family: Provider, e: &ModelEntry) -> Value {
    match family {
        Provider::OpenAi => json!({
            "id": e.id,
            "object": "model",
            "created": 0,
            "owned_by": "GPROXY",
        }),
        Provider::Claude => json!({
            "id": e.id,
            "type": "model",
            "display_name": e.display_name.as_deref().unwrap_or(&e.id),
            "created_at": "1970-01-01T00:00:00Z",
        }),
        Provider::Gemini => {
            let mut m = json!({ "name": wire_id(family, &e.id) });
            if let Some(d) = &e.display_name {
                m["displayName"] = json!(d);
            }
            m
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

/// The wire id of an existing list element (`id` / `name` per family).
fn entry_id(family: Provider, m: &Value) -> Option<String> {
    let key = match family {
        Provider::Gemini => "name",
        _ => "id",
    };
    m.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn to_bytes(v: &Value) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("json! value serializes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            display_name: None,
        }
    }

    #[test]
    fn merge_into_openai_list_dedups_and_appends() {
        let upstream = render_model_list(Provider::OpenAi, &[entry("gpt-a"), entry("gpt-b")]);
        let merged = merge_into_list(
            Provider::OpenAi,
            upstream,
            &[entry("gpt-b"), entry("manual-x")],
        );
        let v: Value = serde_json::from_slice(&merged).unwrap();
        let ids: Vec<&str> = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["gpt-a", "gpt-b", "manual-x"]);
        assert_eq!(v["object"], "list");

        // unparsable upstream body comes back untouched
        let garbage = Bytes::from_static(b"not json");
        assert_eq!(
            merge_into_list(Provider::OpenAi, garbage.clone(), &[entry("x")]),
            garbage
        );
    }
}
