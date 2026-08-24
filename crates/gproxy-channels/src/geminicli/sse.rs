use gproxy_channel_api::{StreamCtx, StreamDecoder};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    crate::shared::code_assist::decoder(ctx)
}
