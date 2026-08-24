use gproxy_channel_api::{NormalizedUsage, UsageCtx};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    crate::shared::openai::usage_from_body(ctx)
}
