use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, WireFamily};
use serde_json::Value;

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if is_anthropic(ctx.key) {
        if ctx.upstream_model.is_empty() {
            return Ok(ctx.body.clone());
        }
        let mut value = crate::shared::claude::hygiene::json_object(ctx.body)?;
        value
            .as_object_mut()
            .expect("JSON object was validated")
            .insert("model".into(), Value::String(ctx.upstream_model.into()));
        return serde_json::to_vec(&value)
            .map(Bytes::from)
            .map_err(|error| ChannelError::Prepare(error.to_string()));
    }
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )
}

pub(super) fn path(ctx: &PrepareCtx<'_>, mode: super::auth::Mode) -> String {
    let path = if ctx.key.operation == Operation::GetModel && !ctx.upstream_model.is_empty() {
        format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        )
    } else {
        ctx.path.into()
    };
    if mode == super::auth::Mode::Oauth {
        path.strip_prefix("/v1").unwrap_or(&path).into()
    } else {
        path
    }
}

pub(super) fn endpoint(key: gproxy_protocol::OperationKey) -> Option<&'static str> {
    if key == gproxy_protocol::OperationKey::family(Operation::ListModels, WireFamily::OpenAi) {
        Some("openai_list_models")
    } else if key
        == gproxy_protocol::OperationKey::family(Operation::CountTokens, WireFamily::Claude)
    {
        Some("claude_count_tokens")
    } else if key
        == gproxy_protocol::OperationKey::family(Operation::CreateEmbedding, WireFamily::OpenAi)
    {
        Some("openai_embeddings")
    } else {
        match key.kind {
            OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
                Some("openai_chat_completions")
            }
            OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses) => {
                Some("openai_responses")
            }
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
                Some("claude_messages")
            }
            _ => None,
        }
    }
}

pub(super) fn is_anthropic(key: gproxy_protocol::OperationKey) -> bool {
    key.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        || key == gproxy_protocol::OperationKey::family(Operation::CountTokens, WireFamily::Claude)
}
