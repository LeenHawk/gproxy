use gproxy_channel_api::{NormalizedUsage, UsageCtx};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    crate::xai::usage::from_body(ctx)
}
