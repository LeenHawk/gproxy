use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind, WireFamily};
use serde_json::Value;

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if is_openai(ctx.key) {
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

pub(super) fn path(ctx: &PrepareCtx<'_>) -> String {
    if ctx.key == family(Operation::GetModel, WireFamily::OpenAi) && !ctx.upstream_model.is_empty()
    {
        format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        )
    } else {
        ctx.path.into()
    }
}

pub(super) fn endpoint(key: gproxy_protocol::OperationKey) -> Option<&'static str> {
    if key == family(Operation::ListModels, WireFamily::OpenAi) {
        Some("openai_list_models")
    } else if key == family(Operation::GetModel, WireFamily::OpenAi) {
        Some("openai_get_model")
    } else if key == family(Operation::CountTokens, WireFamily::Claude) {
        Some("claude_count_tokens")
    } else if key == family(Operation::CreateEmbedding, WireFamily::OpenAi) {
        Some("openai_embeddings")
    } else if is_kind(key, ContentGenerationKind::OpenAiChat) {
        Some("openai_chat_completions")
    } else if is_kind(key, ContentGenerationKind::OpenAiResponses) {
        Some("openai_responses")
    } else if is_kind(key, ContentGenerationKind::ClaudeMessages) {
        Some("claude_messages")
    } else {
        None
    }
}

pub(super) fn is_claude(key: gproxy_protocol::OperationKey) -> bool {
    key == family(Operation::CountTokens, WireFamily::Claude)
        || is_kind(key, ContentGenerationKind::ClaudeMessages)
}

fn is_openai(key: gproxy_protocol::OperationKey) -> bool {
    key == family(Operation::CreateEmbedding, WireFamily::OpenAi)
        || is_kind(key, ContentGenerationKind::OpenAiChat)
        || is_kind(key, ContentGenerationKind::OpenAiResponses)
}

fn is_kind(key: gproxy_protocol::OperationKey, kind: ContentGenerationKind) -> bool {
    key.kind() == OperationKind::ContentGeneration(kind)
}

const fn family(operation: Operation, family: WireFamily) -> gproxy_protocol::OperationKey {
    gproxy_protocol::OperationKey::family(operation, family)
}
