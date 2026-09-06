use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use http::Method;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AuthKind {
    OpenAi,
    Claude,
}

pub(super) struct Target {
    pub method: Method,
    pub path: String,
    pub endpoint: &'static str,
    pub auth: AuthKind,
}

pub(super) fn target(ctx: &PrepareCtx<'_>) -> Result<Target, ChannelError> {
    let key = ctx.key;
    if key == family(Operation::ListModels, WireFamily::OpenAi) {
        return Ok(get("/openai/v1/models".into(), "openai_list_models"));
    }
    if key.kind() == OperationKind::Family(WireFamily::OpenAi)
        && (key.operation().group() == gproxy_protocol::OperationGroup::Video
            || key.operation() == Operation::CompactContent)
    {
        return Ok(Target {
            method: ctx.method.clone(),
            path: format!("/openai{}", ctx.path),
            endpoint: gproxy_channel_api::endpoint_override_key(key)
                .expect("video and compact operations have endpoint keys"),
            auth: AuthKind::OpenAi,
        });
    }
    let model = required_model(ctx)?;
    if key == family(Operation::GetModel, WireFamily::OpenAi) {
        return Ok(get(
            format!(
                "/openai/v1/models/{}",
                crate::shared::http::encode_component(model)
            ),
            "openai_get_model",
        ));
    }
    if key == family(Operation::CountTokens, WireFamily::Claude) {
        return Ok(post(
            "/anthropic/v1/messages/count_tokens".into(),
            "claude_count_tokens",
            AuthKind::Claude,
        ));
    }
    if is_content(key, ContentGenerationKind::OpenAiChat) {
        return Ok(post(
            "/openai/v1/chat/completions".into(),
            "openai_chat_completions",
            AuthKind::OpenAi,
        ));
    }
    if is_content(key, ContentGenerationKind::OpenAiResponses) {
        return Ok(post(
            "/openai/v1/responses".into(),
            "openai_responses",
            AuthKind::OpenAi,
        ));
    }
    if is_content(key, ContentGenerationKind::ClaudeMessages) {
        return Ok(post(
            "/anthropic/v1/messages".into(),
            "claude_messages",
            AuthKind::Claude,
        ));
    }
    let (path, endpoint) = match key.operation() {
        Operation::CreateEmbedding => ("/openai/v1/embeddings".into(), "openai_embeddings"),
        Operation::CreateImage => ("/openai/v1/images/generations".into(), "image_generations"),
        Operation::EditImage => (
            format!(
                "/openai/deployments/{}/images/edits",
                crate::shared::http::encode_component(model)
            ),
            "image_edits",
        ),
        _ => {
            return Err(ChannelError::Prepare(
                "operation is unsupported by Azure".into(),
            ));
        }
    };
    Ok(post(path, endpoint, AuthKind::OpenAi))
}

pub(super) fn is_claude(key: OperationKey) -> bool {
    key.kind() == OperationKind::Family(WireFamily::Claude)
        || key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

fn required_model<'a>(ctx: &'a PrepareCtx<'_>) -> Result<&'a str, ChannelError> {
    (!ctx.upstream_model.trim().is_empty())
        .then_some(ctx.upstream_model.trim())
        .ok_or_else(|| ChannelError::Prepare("Azure request has no deployment model".into()))
}

fn get(path: String, endpoint: &'static str) -> Target {
    Target {
        method: Method::GET,
        path,
        endpoint,
        auth: AuthKind::OpenAi,
    }
}

fn post(path: String, endpoint: &'static str, auth: AuthKind) -> Target {
    Target {
        method: Method::POST,
        path,
        endpoint,
        auth,
    }
}

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

fn is_content(key: OperationKey, kind: ContentGenerationKind) -> bool {
    key.kind() == OperationKind::ContentGeneration(kind)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}
