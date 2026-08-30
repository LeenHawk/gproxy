use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::OperationKind;
use serde_json::Value;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    _headers: &mut http::HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    if !matches!(ctx.key.kind, OperationKind::ContentGeneration(_)) {
        return Ok(body);
    }
    let claude = super::model::is_claude(ctx.key);
    let enabled = if claude {
        enabled(ctx.provider_settings, "enable_claude_magic_cache")
    } else {
        enabled(ctx.provider_settings, "enable_openai_magic_cache")
    };
    if !enabled {
        return Ok(body);
    }
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Prepare(format!("OpenCode request JSON: {error}")))?;
    if claude {
        crate::shared::cache::claude(&mut value);
    } else {
        let kind = match ctx.key.kind {
            OperationKind::ContentGeneration(kind) => kind,
            OperationKind::Family(_) => return Ok(body),
        };
        crate::shared::openai::cache::apply(&mut value, kind);
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn enabled(settings: &Value, name: &str) -> bool {
    settings.get(name).and_then(Value::as_bool) == Some(true)
}
