use gproxy_channel_api::{NormalizedUsage, UsageCtx};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if super::model::is_claude(ctx.key) {
        crate::shared::claude::usage::from_body(ctx.response_body)
    } else {
        crate::shared::openai::usage_from_body(ctx)
    }
}
