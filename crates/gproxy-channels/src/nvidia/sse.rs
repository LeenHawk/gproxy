use gproxy_channel_api::{StreamCtx, StreamDecoder};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
        .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
}
