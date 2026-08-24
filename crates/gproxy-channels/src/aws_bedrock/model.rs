use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn response(body: &Bytes, single: bool) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Bedrock model response JSON: {error}")))?;
    if single {
        let details = value
            .as_object_mut()
            .and_then(|root| root.remove("modelDetails"))
            .unwrap_or(value);
        return encode(model(details)?);
    }
    let root = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("Bedrock model list is not an object".into()))?;
    let summaries = root
        .remove("modelSummaries")
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| ChannelError::Observe("Bedrock model list has no summaries".into()))?;
    let data = summaries
        .into_iter()
        .filter(active_text)
        .map(model)
        .collect::<Result<Vec<_>, _>>()?;
    root.insert("object".into(), Value::String("list".into()));
    root.insert("data".into(), Value::Array(data));
    encode(value)
}

fn active_text(value: &Value) -> bool {
    let active = value
        .pointer("/modelLifecycle/status")
        .and_then(Value::as_str)
        .is_none_or(|status| status == "ACTIVE");
    let text = value
        .get("outputModalities")
        .and_then(Value::as_array)
        .is_none_or(|modalities| {
            modalities
                .iter()
                .any(|value| value.as_str() == Some("TEXT"))
        });
    active && text
}

fn model(value: Value) -> Result<Value, ChannelError> {
    let Value::Object(mut model) = value else {
        return Err(ChannelError::Observe(
            "Bedrock model summary is not an object".into(),
        ));
    };
    let id = model
        .get("modelId")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Observe("Bedrock model has no modelId".into()))?
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

fn encode(value: Value) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
