//! The model directory, synthesized from `fetchAvailableModels`.
//!
//! Antigravity's catalogue reply is not a Gemini model list: `models` is an
//! object keyed by model id (or an array), and the rest of the payload names
//! further ids in role-specific fields the editor uses to pick a model for
//! each feature. v3 `antigravity/models.rs` harvested all of them, dropped
//! embedding models, and rendered Gemini `models[]` entries carrying the
//! token limits the payload reported.

use super::Antigravity;
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::code_assist;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::{StatusCode, header};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Fields whose values name models for one editor feature, in both the
/// snake and camel spellings the payload has used (v3 `ID_FIELDS`).
const ID_FIELDS: &[&str] = &[
    "default_agent_model_id",
    "defaultAgentModelId",
    "agent_model_sorts",
    "agentModelSorts",
    "battle_mode_model_sorts",
    "battleModeModelSorts",
    "command_model_ids",
    "commandModelIds",
    "tab_model_ids",
    "tabModelIds",
    "mquery_model_ids",
    "mqueryModelIds",
    "web_search_model_ids",
    "webSearchModelIds",
    "commit_message_model_ids",
    "commitMessageModelIds",
    "audio_transcription_model_ids",
    "audioTranscriptionModelIds",
    "tiered_model_ids",
    "tieredModelIds",
];

/// The generation methods every Code Assist model serves.
const METHODS: [&str; 3] = ["countTokens", "generateContent", "streamGenerateContent"];

fn collect(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::String(id) => {
            let id = code_assist::model_id(id);
            if !id.is_empty() {
                ids.insert(id.to_owned());
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, ids);
            }
        }
        Value::Object(object) => {
            match ["model_id", "modelId", "id", "name"]
                .iter()
                .find_map(|name| object.get(*name).and_then(Value::as_str))
            {
                Some(id) => collect(&Value::String(id.into()), ids),
                // An object without an id is a grouping (`agentModelSorts`
                // entries are `{displayName, groups: [{modelIds}]}`): its
                // lists and nested objects hold ids, its own strings are
                // labels.
                None => {
                    for value in object.values().filter(|value| !value.is_string()) {
                        collect(value, ids);
                    }
                }
            }
        }
        _ => {}
    }
}

fn entry(id: &str, metadata: Option<&Value>) -> Value {
    let mut model = metadata
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    model.insert("name".into(), Value::String(format!("models/{id}")));
    model.insert("baseModelId".into(), Value::String(id.into()));
    model.insert("supportedGenerationMethods".into(), json!(METHODS));
    if !model.contains_key("displayName")
        && let Some(display) = model.get("display_name").cloned()
    {
        model.insert("displayName".into(), display);
    }
    let metadata = metadata.unwrap_or(&Value::Null);
    if let Some(limit) = metadata.get("maxTokens").and_then(Value::as_u64) {
        model.insert("inputTokenLimit".into(), json!(limit));
    }
    if let Some(limit) = metadata
        .get("maxOutputTokens")
        .or_else(|| metadata.get("outputTokenLimit"))
        .and_then(Value::as_u64)
    {
        model.insert("outputTokenLimit".into(), json!(limit));
    }
    Value::Object(model)
}

pub(super) fn catalog(body: &[u8]) -> Result<Vec<Value>, ChannelError> {
    let payload: Value = serde_json::from_slice(body).map_err(|error| {
        code_assist::invalid_response(format!("Antigravity catalogue JSON: {error}"))
    })?;
    if !payload.is_object() {
        return Err(code_assist::invalid_response(
            "Antigravity catalogue is not an object",
        ));
    }
    let mut metadata: BTreeMap<String, Value> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    match payload.get("models") {
        Some(Value::Object(models)) => {
            for (id, value) in models {
                let id = code_assist::model_id(id).to_owned();
                ids.insert(id.clone());
                metadata.insert(id, value.clone());
            }
        }
        Some(Value::Array(models)) => {
            for model in models {
                match model
                    .get("id")
                    .or_else(|| model.get("name"))
                    .and_then(Value::as_str)
                {
                    Some(id) => {
                        let id = code_assist::model_id(id).to_owned();
                        ids.insert(id.clone());
                        metadata.insert(id, model.clone());
                    }
                    None => collect(model, &mut ids),
                }
            }
        }
        _ => {}
    }
    for field in ID_FIELDS {
        if let Some(value) = payload.get(*field) {
            collect(value, &mut ids);
        }
    }
    Ok(ids
        .into_iter()
        .filter(|id| !id.to_ascii_lowercase().contains("embed"))
        .map(|id| {
            let found = metadata.get(&id);
            entry(&id, found)
        })
        .collect())
}

/// The quota buckets the same payload carries, keyed by model id.
pub(super) fn quota_info(body: &[u8]) -> Result<Map<String, Value>, ChannelError> {
    let payload: Value = serde_json::from_slice(body).map_err(|error| {
        code_assist::invalid_response(format!("Antigravity catalogue JSON: {error}"))
    })?;
    Ok(payload
        .get("models")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default())
}

/// The model id a `GET /v1beta/models/{id}` path names.
fn requested(path: &str) -> Result<String, ChannelError> {
    code_assist::request_model(path, None)
        .ok_or_else(|| ChannelError::InvalidConfig("model id missing from request path".into()))
}

pub(super) async fn invoke(
    channel: &Antigravity,
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let wanted = match operation {
        Operation::GetModel => Some(requested(&ctx.request.path)?),
        _ => None,
    };
    let OperationContext {
        provider,
        credential,
        dialect,
        request,
        client,
        endpoint_override,
        ..
    } = ctx;
    let prepared = channel.prepare(PrepareContext {
        provider,
        credential,
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })?;
    let mut response = client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = code_assist::read_body(response.body).await?;
    let models = catalog(&bytes)?;
    let body = match wanted {
        None => code_assist::encode(&json!({ "models": models }), code_assist::invalid_response)?,
        Some(id) => match models
            .into_iter()
            .find(|model| model["baseModelId"] == Value::String(id.clone()))
        {
            Some(model) => code_assist::encode(&model, code_assist::invalid_response)?,
            None => {
                response.status = StatusCode::NOT_FOUND;
                code_assist::encode(
                    &json!({"error": {
                        "code": 404,
                        "status": "NOT_FOUND",
                        "message": format!("Model `{id}` is not available to this account"),
                    }}),
                    code_assist::invalid_response,
                )?
            }
        },
    };
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response.body = HttpBody::Bytes(body);
    Ok(response)
}
