use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn shape_list(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Observe(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("model list is not an object".into()))?;
    if object.contains_key("error") {
        return Ok(body.clone());
    }
    object
        .entry("object")
        .or_insert_with(|| Value::from("list"));
    if let Some(models) = object.get_mut("data").and_then(Value::as_array_mut) {
        for model in models {
            if let Some(model) = model.as_object_mut() {
                model
                    .entry("object")
                    .or_insert_with(|| Value::from("model"));
                if !model.contains_key("owned_by") {
                    let owner = model
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| id.split_once('/'))
                        .map_or_else(|| "openrouter".into(), |(owner, _)| owner.to_owned());
                    model.insert("owned_by".into(), Value::from(owner));
                }
            }
        }
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
