use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use serde_json::Value;

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if super::model::is_claude(ctx.key) {
        return claude(ctx);
    }
    let body = openai_cache(ctx)?;
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        &body,
    )
}

fn openai_cache(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let gproxy_protocol::OperationKind::ContentGeneration(kind) = ctx.key.kind else {
        return Ok(ctx.body.clone());
    };
    if ctx
        .provider_settings
        .get("enable_openai_magic_cache")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Ok(ctx.body.clone());
    }
    let mut value = serde_json::from_slice(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("request body JSON: {error}")))?;
    crate::shared::openai::cache::apply(&mut value, kind);
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
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
