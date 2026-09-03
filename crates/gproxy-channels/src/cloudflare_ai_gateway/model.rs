use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::Value;

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if matches!(
        ctx.key.kind(),
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses
        )
    ) {
        return crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        );
    }
    if !is_claude(ctx.key) || ctx.upstream_model.is_empty() {
        return Ok(ctx.body.clone());
    }
    let mut body = crate::shared::claude::hygiene::json_object(ctx.body)?;
    body.as_object_mut()
        .expect("JSON object was validated")
        .insert("model".into(), Value::String(ctx.upstream_model.into()));
    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn endpoint(key: gproxy_protocol::OperationKey) -> Option<&'static str> {
    match key.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
            Some("openai_chat_completions")
        }
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => {
            Some("openai_responses")
        }
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            Some("claude_messages")
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent,
        )
        | OperationKind::Family(_) => None,
    }
}

pub(super) fn is_claude(key: gproxy_protocol::OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}
