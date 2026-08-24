use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

const VERSION: &str = "bedrock-2023-05-31";
const BETA: &str = "compact-2026-01-12";

pub(super) fn is_request(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/context_management/edits")?
                .as_array()
                .cloned()
        })
        .is_some_and(|edits| {
            edits
                .iter()
                .any(|edit| edit.get("type").and_then(Value::as_str) == Some("compact_20260112"))
        })
}

pub(super) fn request(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Bedrock compact JSON: {error}")))?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Bedrock compact body is not an object".into()))?;
    root.remove("model");
    root.remove("stream");
    root.entry("anthropic_version")
        .or_insert_with(|| Value::String(VERSION.into()));
    let beta = root
        .entry("anthropic_beta")
        .or_insert_with(|| Value::Array(Vec::new()));
    let values = beta
        .as_array_mut()
        .ok_or_else(|| ChannelError::Prepare("anthropic_beta must be an array".into()))?;
    if !values.iter().any(|value| value.as_str() == Some(BETA)) {
        values.push(Value::String(BETA.into()));
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
