use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};

pub(super) fn path(key: OperationKey) -> &'static str {
    match key.operation() {
        Operation::ListModels => "/v3/config",
        Operation::CreateImage => "/v2/images/generations",
        Operation::EditImage => "/v2/images/edits",
        _ => "/v2/chat/completions",
    }
}

pub(super) fn endpoint_name(key: OperationKey) -> Option<&'static str> {
    if matches!(
        key.kind(),
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
    ) {
        return Some("openai_chat_completions");
    }
    match key.operation() {
        Operation::ListModels => Some("openai_list_models"),
        Operation::CreateImage => Some("image_generations"),
        Operation::EditImage => Some("image_edits"),
        _ => None,
    }
}

pub(super) fn body(
    ctx: &PrepareCtx<'_>,
    headers: &mut http::HeaderMap,
) -> Result<Bytes, ChannelError> {
    match ctx.key.operation() {
        Operation::CreateImage | Operation::EditImage => super::shape::request(ctx, headers),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            crate::shared::openai::shape_request(
                ctx.key,
                ctx.stream,
                ctx.upstream_model,
                ctx.headers,
                ctx.body,
            )
        }
        _ => Ok(ctx.body.clone()),
    }
}
