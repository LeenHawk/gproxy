use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use serde_json::Value;

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut http::HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    let openai = matches!(
        ctx.key.kind(),
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses
        )
    );
    let claude = super::model::is_claude(ctx.key);
    if !openai && !claude {
        return Ok(body);
    }
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Prepare(format!("request body JSON: {error}")))?;
    if openai && enabled(ctx.provider_settings, "enable_openai_magic_cache") {
        let kind = match ctx.key.kind() {
            OperationKind::ContentGeneration(kind) => kind,
            OperationKind::Family(_) => return Ok(body),
        };
        crate::shared::openai::cache::apply(&mut value, kind);
    }
    if claude {
        if ctx.key.operation() == Operation::CountTokens {
            crate::shared::claude::hygiene::count_tokens(&value, headers);
        } else {
            crate::shared::claude::hygiene::messages(&mut value, headers);
        }
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn enabled(settings: &Value, name: &str) -> bool {
    settings.get(name).and_then(Value::as_bool) == Some(true)
}
