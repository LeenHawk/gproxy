use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use serde_json::Value;

pub(super) fn path(ctx: &PrepareCtx<'_>) -> String {
    match ctx.key.operation() {
        Operation::GetModel if !ctx.upstream_model.is_empty() => format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        ),
        Operation::CreateSpeech => "/v1/tts".into(),
        Operation::CreateTranscription => "/v1/stt".into(),
        Operation::CreateVideo => "/v1/videos/generations".into(),
        _ => ctx.path.into(),
    }
}

pub(super) fn body(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("xAI model list JSON: {error}")))?;
    let models = value
        .get_mut("data")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ChannelError::Observe("xAI model list has no data array".into()))?;
    let mut changed = false;
    for model in models {
        let Some(object) = model.as_object_mut() else {
            continue;
        };
        if object.get("id").and_then(Value::as_str) != Some("grok-4.6") {
            continue;
        }
        object.insert("display_name".into(), Value::String("Grok 4.6".into()));
        object.insert("context_length".into(), Value::from(500_000));
        object.insert(
            "supported_parameters".into(),
            serde_json::json!(["reasoning"]),
        );
        object.insert("thinking_supported".into(), Value::Bool(true));
        changed = true;
    }
    if !changed {
        return Ok(body.clone());
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
