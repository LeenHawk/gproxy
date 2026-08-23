use bytes::Bytes;
use gproxy_channel_api::{ChannelError, ResponseShapeCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::openai::common::{ListObjectType, ModelObjectType, OpenAiModelId};
use gproxy_protocol::openai::models::{Model, ModelListResponse};
use serde_json::Value;

pub(super) fn shape(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    match ctx.key.operation {
        Operation::ListModels => list(ctx.body),
        Operation::GetModel => get(ctx.body),
        _ => Ok(ctx.body.clone()),
    }
}

fn list(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Decode(format!("Codex model list JSON: {error}")))?;
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| ChannelError::Decode("Codex model list missing models".into()))?;
    let mut rest = value.as_object().cloned().unwrap_or_default();
    rest.remove("models");
    let response = ModelListResponse {
        data: models
            .iter()
            .map(normalize)
            .collect::<Result<Vec<_>, _>>()?,
        object: ListObjectType::List,
        rest,
    };
    serde_json::to_vec(&response)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Decode(error.to_string()))
}

fn get(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Decode(format!("Codex model JSON: {error}")))?;
    let model = if value.get("slug").is_some() || value.get("id").is_some() {
        normalize(&value)?
    } else {
        let value = value
            .get("models")
            .and_then(Value::as_array)
            .and_then(|models| models.first())
            .ok_or_else(|| ChannelError::Decode("Codex model response is empty".into()))?;
        normalize(value)?
    };
    serde_json::to_vec(&model)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Decode(error.to_string()))
}

fn normalize(value: &Value) -> Result<Model, ChannelError> {
    let id = value
        .get("slug")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Decode("Codex model entry missing id".into()))?;
    let max_context_window = positive(value, "max_context_window");
    let mut rest = value.as_object().cloned().unwrap_or_default();
    for name in [
        "slug",
        "id",
        "display_name",
        "context_window",
        "max_context_window",
        "max_output_tokens",
        "supported_reasoning_levels",
    ] {
        rest.remove(name);
    }
    Ok(Model {
        id: OpenAiModelId::from(id),
        created: None,
        display_name: value
            .get("display_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        context_window: max_context_window.or_else(|| positive(value, "context_window")),
        max_context_window,
        max_output_tokens: positive(value, "max_output_tokens"),
        thinking_supported: value
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .map(|levels| !levels.is_empty()),
        object: ModelObjectType::Model,
        owned_by: None,
        rest,
    })
}

fn positive(value: &Value, name: &str) -> Option<u64> {
    value.get(name).and_then(Value::as_u64).filter(|v| *v > 0)
}
