use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use http::HeaderMap;
use serde_json::Value;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    if !super::model::is_messages(ctx.key) && !super::model::is_count_tokens(ctx.key) {
        return Ok(ctx.body.clone());
    }
    let mut body = crate::shared::claude::hygiene::json_object(ctx.body)?;
    if !ctx.upstream_model.is_empty() {
        body.as_object_mut()
            .expect("JSON object was validated")
            .insert("model".into(), Value::String(ctx.upstream_model.into()));
    }
    if super::model::is_messages(ctx.key) {
        crate::shared::claude::hygiene::messages(&mut body, headers);
        crate::shared::claude::fallback::apply(&mut body, headers, ctx.provider_settings);
    } else {
        crate::shared::claude::hygiene::count_tokens(&body, headers);
    }
    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
