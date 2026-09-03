mod media;
mod responses;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use http::HeaderMap;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    if matches!(
        ctx.key.kind(),
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses)
    ) {
        let body = crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        )?;
        return responses::request(&body);
    }
    match ctx.key.operation() {
        Operation::CreateImage
        | Operation::EditImage
        | Operation::CreateSpeech
        | Operation::CreateTranscription
        | Operation::CreateVideo
        | Operation::EditVideo
        | Operation::ExtendVideo => media::request(ctx, headers),
        _ => crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        ),
    }
}

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if ctx.status.is_success()
        && matches!(
            ctx.key.operation(),
            Operation::CreateVideo
                | Operation::RetrieveVideo
                | Operation::EditVideo
                | Operation::ExtendVideo
        )
    {
        media::video_response(ctx.body)
    } else {
        Ok(ctx.body.clone())
    }
}

fn encode(value: serde_json::Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
