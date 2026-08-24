use gproxy_channel_api::{NormalizedUsage, UsageCtx};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    let body =
        crate::shared::code_assist::unwrap(&bytes::Bytes::copy_from_slice(ctx.response_body))
            .ok()?;
    crate::shared::gemini::usage::from_body(UsageCtx {
        key: ctx.key,
        request_body: ctx.request_body,
        response_headers: ctx.response_headers,
        response_body: &body,
    })
}
