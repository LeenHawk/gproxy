use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )
}
