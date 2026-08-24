use bytes::Bytes;
use gproxy_channel_api::{ChannelError, ResponseShapeCtx};
use gproxy_protocol::Operation;

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    if ctx.key.operation == Operation::ListModels {
        return super::models::response(ctx.body);
    }
    let body = crate::shared::code_assist::unwrap(ctx.body)?;
    crate::shared::gemini::vertex::normalize_content(&body)
}
