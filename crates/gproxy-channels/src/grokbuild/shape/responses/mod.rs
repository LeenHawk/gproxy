mod reasoning;
mod session;
mod tools;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn request(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Responses body JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Responses body must be an object".into()))?;
    object.remove("previous_response_id");
    for name in [
        "metadata",
        "prompt_cache_retention",
        "safety_identifier",
        "stream_options",
    ] {
        object.remove(name);
    }
    if object
        .get("top_p")
        .and_then(Value::as_f64)
        .is_some_and(|value| value <= 0.0)
    {
        object.remove("top_p");
    }
    tools::normalize(object);
    reasoning::sanitize(object);
    session::ensure(object)?;
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
