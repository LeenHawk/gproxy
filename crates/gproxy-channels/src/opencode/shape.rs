use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::OperationKind;
use serde_json::Value;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    _headers: &mut http::HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    if !matches!(ctx.key.kind(), OperationKind::ContentGeneration(_)) {
        return Ok(body);
    }
    // Claude magic markers are shaped once for every Claude target, centrally.
    // Only the OpenAI side is still a per-provider opt-in here.
    let kind = match ctx.key.kind() {
        OperationKind::ContentGeneration(kind) => kind,
        OperationKind::Family(_) => return Ok(body),
    };
    if super::model::is_claude(ctx.key)
        || !enabled(ctx.provider_settings, "enable_openai_magic_cache")
    {
        return Ok(body);
    }
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Prepare(format!("OpenCode request JSON: {error}")))?;
    crate::shared::openai::cache::apply(&mut value, kind);
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn enabled(settings: &Value, name: &str) -> bool {
    settings.get(name).and_then(Value::as_bool) == Some(true)
}
