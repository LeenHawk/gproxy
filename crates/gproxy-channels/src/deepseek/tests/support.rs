use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::HeaderMap;
use serde_json::{Value, json};

pub(super) fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

pub(super) fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

pub(super) fn prepare(
    key: OperationKey,
    model: &str,
    headers: &HeaderMap,
    body: &Bytes,
    settings: &Value,
) -> PreparedRequest {
    let secret = json!({"api_key":"deepseek-key"});
    super::super::DeepSeekChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: key.operation() == Operation::StreamGenerateContent,
            method: &http::Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers,
            body,
            upstream_model: model,
            provider_settings: settings,
            secret: &secret,
        })
        .unwrap()
}
