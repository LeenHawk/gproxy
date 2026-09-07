use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};
use serde_json::{Value, json};

const VERSION: &str = "bedrock-2023-05-31";

pub(super) fn enabled(ctx: &PrepareCtx<'_>) -> bool {
    ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        && (gproxy_channel_api::claude_fallback_setting(ctx.provider_settings).is_some()
            || native(ctx.body)
            || ctx
                .headers
                .get("anthropic-beta")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.contains("fallback-credit-")))
}

pub(super) fn native(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .is_some_and(|body| {
            body["anthropic_version"] == VERSION || gproxy_channel_api::has_fallback_credit(&body)
        })
}

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let mut body = crate::shared::claude::hygiene::json_object(ctx.body)?;
    let object = body.as_object_mut().expect("validated object");
    object.remove("model");
    object.remove("stream");
    object.remove("fallbacks");
    object.insert("anthropic_version".into(), json!(VERSION));
    let mut betas = object
        .get("anthropic_beta")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for beta in ctx
        .headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|beta| !beta.is_empty() && !beta.starts_with("server-side-fallback-"))
    {
        if !betas.iter().any(|value| value.as_str() == Some(beta)) {
            betas.push(json!(beta));
        }
    }
    if !betas.is_empty() {
        object.insert("anthropic_beta".into(), json!(betas));
    }
    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}
