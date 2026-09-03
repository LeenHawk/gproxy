use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::{ContentGenerationKind, OperationKey, OperationKind};
use http::HeaderMap;
use serde_json::Value;

pub(super) fn request(
    key: OperationKey,
    settings: &Value,
    headers: &mut HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    let openai = matches!(
        key.kind(),
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses
        )
    ) && enabled(settings, "enable_openai_magic_cache");
    let claude =
        key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages);
    let fallback = claude && crate::shared::claude::fallback::enabled(settings);
    if !openai && !fallback {
        return Ok(body);
    }
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Prepare(format!("request body JSON: {error}")))?;
    if openai {
        let kind = match key.kind() {
            OperationKind::ContentGeneration(kind) => kind,
            OperationKind::Family(_) => return Ok(body),
        };
        crate::shared::openai::cache::apply(&mut value, kind);
    }
    if fallback {
        crate::shared::claude::fallback::apply(&mut value, headers, settings);
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn enabled(settings: &Value, name: &str) -> bool {
    settings.get(name).and_then(Value::as_bool) == Some(true)
}
