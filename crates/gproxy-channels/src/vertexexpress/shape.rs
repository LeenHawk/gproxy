use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let rewritten =
        crate::shared::gemini::model::rewrite(ctx.key.operation, ctx.body, ctx.upstream_model)?;
    let mut value: serde_json::Value = serde_json::from_slice(&rewritten)
        .map_err(|error| ChannelError::Prepare(format!("Gemini request JSON: {error}")))?;
    value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Gemini request must be an object".into()))?
        .remove("store");
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
