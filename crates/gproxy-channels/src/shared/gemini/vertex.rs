use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(crate) fn normalize_content(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(json_error)?;
    if let Some(candidates) = value.get_mut("candidates").and_then(Value::as_array_mut) {
        for candidate in candidates {
            let Some(metadata) = candidate
                .get_mut("citationMetadata")
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            if let Some(citations) = metadata.remove("citations") {
                metadata.entry("citationSources").or_insert(citations);
            }
        }
    }
    if let Some(reason) = value.pointer_mut("/promptFeedback/blockReason")
        && reason.as_str() == Some("BLOCKED_REASON_UNSPECIFIED")
    {
        *reason = Value::String("BLOCK_REASON_UNSPECIFIED".into());
    }
    encode(value)
}

pub(crate) fn normalize_model(body: &Bytes, list: bool) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body).map_err(json_error)?;
    if list {
        let root = value
            .as_object_mut()
            .ok_or_else(|| ChannelError::Observe("Vertex model list is not an object".into()))?;
        if !root.contains_key("models")
            && let Some(models) = root.remove("publisherModels")
        {
            root.insert("models".into(), models);
        }
        if let Some(models) = root.get_mut("models").and_then(Value::as_array_mut) {
            for model in models {
                normalize_model_name(model);
            }
        }
    } else {
        normalize_model_name(&mut value);
    }
    encode(value)
}

fn normalize_model_name(model: &mut Value) {
    let Some(name) = model.get_mut("name") else {
        return;
    };
    let Some(current) = name.as_str() else {
        return;
    };
    if let Some((_, id)) = current.rsplit_once("/models/") {
        *name = Value::String(format!("models/{id}"));
    }
}

fn encode(value: Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(format!("Vertex response JSON: {error}"))
}
