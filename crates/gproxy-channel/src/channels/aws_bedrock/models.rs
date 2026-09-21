//! The control plane's foundation-model directory rendered as an OpenAI model
//! list (v3 `aws_bedrock/model.rs`).
//!
//! `ListFoundationModels` answers `{"modelSummaries": [...]}` and
//! `GetFoundationModel` answers `{"modelDetails": {...}}`; neither is an
//! OpenAI shape, so the channel declares `openai` natively and rewrites the
//! reply. Retired models and models that cannot emit text are dropped: they
//! are not routable targets. Every AWS field is kept alongside the OpenAI
//! ones so nothing a caller might want is lost.

use serde_json::Value;

use crate::channel::ChannelError;

pub(super) fn rewrite(body: &[u8], single: bool) -> Result<Vec<u8>, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid(format!("model response JSON: {error}")))?;
    if single {
        let details = value
            .as_object_mut()
            .and_then(|root| root.remove("modelDetails"))
            .unwrap_or(value);
        return encode(&model(details)?);
    }
    let root = value
        .as_object_mut()
        .ok_or_else(|| invalid("the model list is not an object"))?;
    let summaries = root
        .remove("modelSummaries")
        .and_then(|value| match value {
            Value::Array(items) => Some(items),
            _ => None,
        })
        .ok_or_else(|| invalid("the model list has no modelSummaries"))?;
    let data = summaries
        .into_iter()
        .filter(active_text)
        .map(model)
        .collect::<Result<Vec<_>, _>>()?;
    root.insert("object".into(), Value::String("list".into()));
    root.insert("data".into(), Value::Array(data));
    encode(&value)
}

/// Keep a model unless AWS says it is retired or says it emits no text.
/// A summary that reports neither fact is kept.
fn active_text(value: &Value) -> bool {
    let active = value
        .pointer("/modelLifecycle/status")
        .and_then(Value::as_str)
        .is_none_or(|status| status == "ACTIVE");
    let text = value
        .get("outputModalities")
        .and_then(Value::as_array)
        .is_none_or(|modalities| modalities.iter().any(|value| value == "TEXT"));
    active && text
}

fn model(value: Value) -> Result<Value, ChannelError> {
    let Value::Object(mut model) = value else {
        return Err(invalid("a model summary is not an object"));
    };
    let id = model
        .get("modelId")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("a model has no modelId"))?
        .to_owned();
    let owner = model
        .get("providerName")
        .and_then(Value::as_str)
        .unwrap_or("AWS Bedrock")
        .to_owned();
    if let Some(display) = model.get("modelName").cloned() {
        model.insert("display_name".into(), display);
    }
    model.insert("id".into(), Value::String(id));
    model.insert("object".into(), Value::String("model".into()));
    model.insert("owned_by".into(), Value::String(owner));
    Ok(Value::Object(model))
}

fn encode(value: &Value) -> Result<Vec<u8>, ChannelError> {
    serde_json::to_vec(value).map_err(|error| invalid(error.to_string()))
}

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("AWS Bedrock: {}", message.into()))
}
