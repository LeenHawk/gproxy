use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};
use serde_json::Value;

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if super::model::is_chat(ctx.key) {
        let value: Value = serde_json::from_slice(ctx.response_body).ok()?;
        return value.get("usage").and_then(from_chat_usage);
    }
    if super::model::is_claude(ctx.key) {
        crate::shared::claude::usage::from_body(ctx.response_body)
    } else {
        crate::shared::openai::usage_from_body(ctx)
    }
}

pub(super) fn from_chat_usage(value: &Value) -> Option<NormalizedUsage> {
    let body = serde_json::to_vec(&serde_json::json!({"usage":value})).ok()?;
    let headers = http::HeaderMap::new();
    let request_body = bytes::Bytes::new();
    let mut usage = crate::shared::openai::usage_from_body(UsageCtx {
        key: OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        request_body: &request_body,
        response_headers: &headers,
        response_body: &body,
    })?;
    if let Some(cached) = value.get("prompt_cache_hit_tokens").and_then(Value::as_u64) {
        usage.cached_input_tokens = cached;
    }
    Some(usage)
}
