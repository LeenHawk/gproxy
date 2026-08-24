use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use serde_json::Value;

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if super::model::is_claude(ctx.key) {
        return claude(ctx);
    }
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )
}

fn claude(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let mut value = serde_json::from_slice::<Value>(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("request body is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("request body must be a JSON object".into()))?;
    object.insert(
        "model".into(),
        Value::String(ctx.upstream_model.trim().into()),
    );
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
