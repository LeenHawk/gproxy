use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::{Value, json};

pub(super) fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

pub(super) fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

pub(super) fn prepare(
    key: OperationKey,
    model: &str,
    body: &Bytes,
    settings: &Value,
) -> PreparedRequest {
    let secret = json!({"api_key":"dashscope-key"});
    super::super::DashScopeChannel
        .prepare(PrepareCtx {
            key,
            stream: key.operation() == Operation::StreamGenerateContent,
            method: &http::Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers: &http::HeaderMap::new(),
            body,
            upstream_model: model,
            provider_settings: settings,
            secret: &secret,
        })
        .unwrap()
}
