mod audio;
mod image;
mod video;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::Operation;
use http::HeaderMap;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    match ctx.key.operation() {
        Operation::CreateImage | Operation::EditImage => image::request(ctx, headers),
        Operation::CreateSpeech | Operation::CreateTranscription => audio::request(ctx, headers),
        Operation::CreateVideo | Operation::EditVideo | Operation::ExtendVideo => {
            video::request(ctx)
        }
        _ => super::model::body(ctx),
    }
}

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if ctx.status.is_success() && ctx.key.operation() == Operation::ListModels {
        super::model::response(ctx.body)
    } else if ctx.status.is_success()
        && matches!(
            ctx.key.operation(),
            Operation::CreateVideo
                | Operation::RetrieveVideo
                | Operation::EditVideo
                | Operation::ExtendVideo
        )
    {
        video::response(ctx.body)
    } else {
        Ok(ctx.body.clone())
    }
}

fn json_object(
    body: &[u8],
    label: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, ChannelError> {
    serde_json::from_slice::<serde_json::Value>(body)
        .map_err(|error| ChannelError::Prepare(format!("{label} body JSON: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare(format!("{label} body must be an object")))
}

fn encode(value: serde_json::Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
