use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};
use serde_json::Value;

pub(super) fn path(key: OperationKey) -> &'static str {
    match key.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => "/chat/completions",
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => "/responses",
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => "/messages",
        _ => "/models",
    }
}

pub(super) fn endpoint_name(key: OperationKey) -> Option<&'static str> {
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
        _ if key.operation() == Operation::ListModels => Some("openai_list_models"),
        _ => None,
    }
}

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    match ctx.key.kind() {
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses,
        ) => crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        ),
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            let mut body = crate::shared::claude::hygiene::json_object(ctx.body)?;
            if !ctx.upstream_model.is_empty() {
                body.as_object_mut()
                    .expect("Claude object was validated")
                    .insert("model".into(), Value::String(ctx.upstream_model.into()));
            }
            serde_json::to_vec(&body)
                .map(Bytes::from)
                .map_err(|error| ChannelError::Prepare(error.to_string()))
        }
        _ => Ok(ctx.body.clone()),
    }
}

pub(super) fn is_claude(key: OperationKey) -> bool {
    key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}
