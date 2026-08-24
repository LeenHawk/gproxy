use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;

pub(super) fn path(ctx: &PrepareCtx<'_>) -> String {
    match ctx.key.operation {
        Operation::GetModel if !ctx.upstream_model.is_empty() => format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        ),
        Operation::CreateSpeech => "/v1/tts".into(),
        Operation::CreateTranscription => "/v1/stt".into(),
        Operation::CreateVideo => "/v1/videos/generations".into(),
        _ => ctx.path.into(),
    }
}

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )
}
