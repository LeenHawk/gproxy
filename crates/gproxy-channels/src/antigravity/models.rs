use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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
    "image_generation_model_ids",
    "imageGenerationModelIds",
    "tiered_model_ids",
    "tieredModelIds",
];

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("model response JSON: {error}")))?;
    let Value::Object(_) = &payload else {
        return Err(ChannelError::Observe(
            "model response is not an object".into(),
        ));
    };
    let mut metadata = BTreeMap::new();
    let mut ids = BTreeSet::new();
    if let Some(models) = payload.get("models").and_then(Value::as_object) {
        for (id, value) in models {
            let id = normalize(id);
            ids.insert(id.clone());
            metadata.insert(id, value.clone());
        }
    } else if let Some(models) = payload.get("models").and_then(Value::as_array) {
        for model in models {
            if let Some(id) = model
                .get("id")
                .or_else(|| model.get("name"))
                .and_then(Value::as_str)
            {
                let id = normalize(id);
                ids.insert(id.clone());
                metadata.insert(id, model.clone());
            } else {
                collect(model, &mut ids);
            }
        }
    }
    for field in ID_FIELDS {
        if let Some(value) = payload.get(*field) {
            collect(value, &mut ids);
        }
    }
    let models = ids
        .into_iter()
        .filter(|id| !id.to_ascii_lowercase().contains("embed"))
        .map(|id| entry(&id, metadata.get(&id)))
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({"models":models}))
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}

fn collect(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::String(id) => {
            let id = normalize(id);
            if !id.is_empty() {
                ids.insert(id);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, ids);
            }
        }
        Value::Object(object) => {
            if let Some(id) = ["model_id", "modelId", "id", "name"]
                .iter()
                .find_map(|name| object.get(*name).and_then(Value::as_str))
            {
                collect(&Value::String(id.into()), ids);
            } else {
                for value in object.values() {
                    collect(value, ids);
                }
            }
        }
        _ => {}
    }
}

fn normalize(id: &str) -> String {
    id.trim()
        .trim_start_matches('/')
        .trim_start_matches("models/")
        .into()
}

fn entry(id: &str, metadata: Option<&Value>) -> Value {
    let mut model = metadata
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    model.insert("name".into(), Value::String(format!("models/{id}")));
    model.insert("baseModelId".into(), Value::String(id.into()));
    model.insert(
        "supportedGenerationMethods".into(),
        json!(["countTokens", "generateContent", "streamGenerateContent"]),
    );
    if model.get("displayName").is_none()
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
