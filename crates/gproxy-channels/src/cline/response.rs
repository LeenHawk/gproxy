use std::collections::BTreeSet;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, ResponseShapeCtx};
use gproxy_protocol::Operation;
use serde_json::{Map, Value};

pub(super) fn shape(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    if ctx.key.operation() == Operation::ListModels {
        model_list(ctx.body)
    } else {
        unwrap_chat(ctx.body)
    }
}

pub(super) fn unwrap_chat(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Cline response JSON: {error}")))?;
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Ok(body.clone());
    }
    let data = value
        .get("data")
        .ok_or_else(|| ChannelError::Observe("Cline response has no data".into()))?;
    serde_json::to_vec(data)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}

fn model_list(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Cline model JSON: {error}")))?;
    if value.get("error").is_some() {
        return Ok(body.clone());
    }
    let mut seen = BTreeSet::new();
    let mut data = Vec::new();
    for group in ["free", "clinePass"] {
        for model in value
            .get(group)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(mut model) = model.as_object().cloned() else {
                continue;
            };
            let Some(id) = model.get("id").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            model.insert("object".into(), Value::String("model".into()));
            model.insert("cline_group".into(), Value::String(group.into()));
            data.push(Value::Object(model));
        }
    }
    let mut output = Map::new();
    output.insert("object".into(), Value::String("list".into()));
    output.insert("data".into(), Value::Array(data));
    serde_json::to_vec(&Value::Object(output))
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
