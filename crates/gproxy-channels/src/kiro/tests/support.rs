use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, PreparedRequest};
use gproxy_protocol::OperationKey;
use serde_json::Value;

pub(super) fn prepare(
    key: OperationKey,
    model: &str,
    body: &Bytes,
    secret: &Value,
    settings: &Value,
) -> PreparedRequest {
    super::super::KiroChannel
        .prepare(PrepareCtx {
            key,
            stream: key.operation() == gproxy_protocol::Operation::StreamGenerateContent,
            method: &http::Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers: &http::HeaderMap::new(),
            body,
            upstream_model: model,
            provider_settings: settings,
            secret,
        })
        .unwrap()
}

pub(super) fn event(name: &str, payload: &[u8]) -> Bytes {
    let mut headers = Vec::new();
    for (key, value) in [
        (":message-type", "event"),
        (":event-type", name),
        (":content-type", "application/json"),
    ] {
        headers.push(key.len() as u8);
        headers.extend_from_slice(key.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let total = 12 + headers.len() + payload.len() + 4;
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(total as u32).to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32fast::hash(&frame[..8]).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    frame.extend_from_slice(&crc32fast::hash(&frame).to_be_bytes());
    Bytes::from(frame)
}

pub(super) fn append(output: &mut Vec<u8>, frames: Vec<gproxy_channel_api::Frame>) {
    for frame in frames {
        output.extend_from_slice(&frame.0);
    }
}
