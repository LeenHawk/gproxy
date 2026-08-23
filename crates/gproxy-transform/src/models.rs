use bytes::Bytes;
use serde_json::{Map, Value};

use crate::TransformError;

pub(crate) fn claude_to_openai_response(body: Bytes) -> Result<Bytes, TransformError> {
    let mut value: Value = serde_json::from_slice(&body)?;
    if let Some(data) = value.get_mut("data").and_then(Value::as_array_mut) {
        for model in data {
            *model = claude_to_openai_model(take_object(model, "Claude model")?);
        }
        let object = value
            .as_object_mut()
            .ok_or_else(|| TransformError::shape("Claude models", "root must be an object"))?;
        object.insert("object".into(), Value::String("list".into()));
        object.remove("first_id");
        object.remove("last_id");
        object.remove("has_more");
    } else {
        value = claude_to_openai_model(take_object(&mut value, "Claude model")?);
    }
    encode(value)
}

pub(crate) fn openai_to_claude_response(body: Bytes) -> Result<Bytes, TransformError> {
    let mut value: Value = serde_json::from_slice(&body)?;
    if let Some(data) = value.get_mut("data").and_then(Value::as_array_mut) {
        for model in data.iter_mut() {
            *model = openai_to_claude_model(take_object(model, "OpenAI model")?);
        }
        let first = data.first().and_then(|model| model.get("id")).cloned();
        let last = data.last().and_then(|model| model.get("id")).cloned();
        let object = value
            .as_object_mut()
            .ok_or_else(|| TransformError::shape("OpenAI models", "root must be an object"))?;
        object.remove("object");
        object.insert("has_more".into(), Value::Bool(false));
        object.insert(
            "first_id".into(),
            first.unwrap_or(Value::String(String::new())),
        );
        object.insert(
            "last_id".into(),
            last.unwrap_or(Value::String(String::new())),
        );
    } else {
        value = openai_to_claude_model(take_object(&mut value, "OpenAI model")?);
    }
    encode(value)
}

fn claude_to_openai_model(mut model: Map<String, Value>) -> Value {
    model.remove("type");
    model.remove("created_at");
    model.remove("allowed_fallback_models");
    if let Some(value) = model.remove("max_input_tokens") {
        model.insert("context_window".into(), value);
    }
    if let Some(value) = model.remove("max_tokens") {
        model.insert("max_output_tokens".into(), value);
    }
    if let Some(supported) = model
        .get("capabilities")
        .and_then(|value| value.pointer("/thinking/supported"))
        .cloned()
    {
        model.insert("thinking_supported".into(), supported);
    }
    model.remove("capabilities");
    model.insert("object".into(), Value::String("model".into()));
    model.insert("owned_by".into(), Value::String("anthropic".into()));
    Value::Object(model)
}

fn openai_to_claude_model(mut model: Map<String, Value>) -> Value {
    model.remove("object");
    model.remove("created");
    model.remove("owned_by");
    if let Some(value) = model.remove("context_window") {
        model.insert("max_input_tokens".into(), value);
    }
    if let Some(value) = model.remove("max_output_tokens") {
        model.insert("max_tokens".into(), value);
    }
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    model
        .entry("display_name")
        .or_insert_with(|| Value::String(id));
    model.insert("type".into(), Value::String("model".into()));
    model
        .entry("created_at")
        .or_insert_with(|| Value::String("1970-01-01T00:00:00Z".into()));
    Value::Object(model)
}

fn take_object(
    value: &mut Value,
    name: &'static str,
) -> Result<Map<String, Value>, TransformError> {
    match std::mem::take(value) {
        Value::Object(object) => Ok(object),
        _ => Err(TransformError::shape(name, "value must be an object")),
    }
}

fn encode(value: Value) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(&value)?))
}
