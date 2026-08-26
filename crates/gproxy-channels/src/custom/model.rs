use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum AuthKind {
    OpenAi,
    Claude,
    Gemini,
}

pub(super) fn path(ctx: &PrepareCtx<'_>) -> String {
    if ctx.key.operation == Operation::GetModel && !ctx.upstream_model.is_empty() {
        let prefix = match ctx.key.kind {
            OperationKind::Family(WireFamily::Gemini) => "/v1beta/models/",
            _ => "/v1/models/",
        };
        return format!(
            "{prefix}{}",
            crate::shared::http::encode_component(model_id(ctx.upstream_model))
        );
    }
    if ctx.key.kind == OperationKind::Family(WireFamily::Gemini)
        || ctx.key.kind
            == OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
    {
        return gemini_path(ctx.path, ctx.upstream_model);
    }
    ctx.path.into()
}

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    match ctx.key.kind {
        OperationKind::Family(WireFamily::Claude)
        | OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            claude_body(ctx)
        }
        OperationKind::Family(WireFamily::Gemini)
        | OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            crate::shared::gemini::model::rewrite(ctx.key.operation, ctx.body, ctx.upstream_model)
        }
        OperationKind::Family(WireFamily::OpenAi)
        | OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses,
        ) => crate::shared::openai::model::shape(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        ),
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponsesWebSocket) => Err(
            ChannelError::Prepare("custom channel does not support Responses WebSocket".into()),
        ),
    }
}

pub(super) fn auth(key: OperationKey) -> AuthKind {
    match key.kind {
        OperationKind::Family(WireFamily::Claude)
        | OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            AuthKind::Claude
        }
        OperationKind::Family(WireFamily::Gemini)
        | OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            AuthKind::Gemini
        }
        _ => AuthKind::OpenAi,
    }
}

pub(super) fn endpoint(key: OperationKey) -> Option<&'static str> {
    gproxy_channel_api::endpoint_override_key(key)
}

fn claude_body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if !matches!(
        ctx.key.operation,
        Operation::CountTokens | Operation::GenerateContent | Operation::StreamGenerateContent
    ) || ctx.upstream_model.is_empty()
    {
        return Ok(ctx.body.clone());
    }
    let mut value: Value = serde_json::from_slice(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("Claude request JSON: {error}")))?;
    value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Claude request is not an object".into()))?
        .insert("model".into(), Value::String(ctx.upstream_model.into()));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn gemini_path(path: &str, model: &str) -> String {
    let Some(rest) = path.strip_prefix("/v1beta/models/") else {
        return path.into();
    };
    let end = rest.find([':', '/']).unwrap_or(rest.len());
    format!(
        "/v1beta/models/{}{}",
        crate::shared::http::encode_component(model_id(model)),
        &rest[end..]
    )
}

fn model_id(model: &str) -> &str {
    crate::shared::gemini::model::model_id(model)
}
