use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Map, Value, json};

pub(super) fn create(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut input: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Vertex video request JSON: {error}")))?;
    let prompt = input
        .remove("prompt")
        .unwrap_or_else(|| Value::String(String::new()));
    input.remove("model");
    let reference = input.remove("input_reference");
    let mut instance = Map::new();
    instance.insert("prompt".into(), prompt);
    if let Some(image) = reference.and_then(reference_image) {
        instance.insert("image".into(), image);
    }

    let mut parameters = input
        .remove("parameters")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    for (source, target) in [
        ("aspect_ratio", "aspectRatio"),
        ("duration_seconds", "durationSeconds"),
        ("generate_audio", "generateAudio"),
        ("resolution", "resolution"),
        ("seed", "seed"),
        ("storage_uri", "storageUri"),
    ] {
        if let Some(value) = input.remove(source) {
            parameters.entry(target).or_insert(value);
        }
    }
    if let Some(seconds) = input.remove("seconds") {
        let duration = seconds
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Value::from)
            .unwrap_or(seconds);
        parameters.entry("durationSeconds").or_insert(duration);
    }
    if let Some(count) = input.remove("n") {
        parameters.entry("sampleCount").or_insert(count);
    }
    encode(json!({
        "instances": [Value::Object(instance)],
        "parameters": Value::Object(parameters),
    }))
}

fn reference_image(reference: Value) -> Option<Value> {
    let url = reference
        .as_str()
        .map(str::to_owned)
        .or_else(|| reference.get("image_url")?.as_str().map(str::to_owned))?;
    if let Some(data) = url.strip_prefix("data:") {
        let (mime, payload) = data.split_once(',')?;
        Some(json!({
            "bytesBase64Encoded": payload,
            "mimeType": mime.strip_suffix(";base64")?,
        }))
    } else {
        url.starts_with("gs://").then(|| json!({ "gcsUri": url }))
    }
}

fn encode(value: Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
