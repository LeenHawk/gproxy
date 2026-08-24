use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Kiro model JSON: {error}")))?;
    let Value::Object(mut root) = value else {
        return Err(ChannelError::Observe(
            "Kiro model response is not an object".into(),
        ));
    };
    let models = root
        .remove("models")
        .or_else(|| root.remove("data"))
        .and_then(|models| models.as_array().cloned())
        .ok_or_else(|| ChannelError::Observe("Kiro model response has no models".into()))?;
    let data: Vec<Value> = models
        .into_iter()
        .filter_map(|model| match model {
            Value::String(id) => Some(serde_json::json!({"id":id,"object":"model"})),
            Value::Object(mut model) => {
                let id = model_id(&model)?.to_owned();
                model.insert("id".into(), Value::String(id));
                model
                    .entry("object")
                    .or_insert_with(|| Value::String("model".into()));
                Some(Value::Object(model))
            }
            _ => None,
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({"object":"list","data":data}))
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}

fn model_id(value: &serde_json::Map<String, Value>) -> Option<&str> {
    ["modelId", "model_id", "id", "name"]
        .into_iter()
        .find_map(|name| value.get(name).and_then(Value::as_str))
}
