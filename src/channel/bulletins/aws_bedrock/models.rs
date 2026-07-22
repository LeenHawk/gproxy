use bytes::Bytes;
use serde_json::{Value, json};

pub(super) fn response(body: Bytes, single: bool) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    if single {
        let details = value.get("modelDetails").unwrap_or(&value);
        return model(details).map_or(body, |model| Bytes::from(model.to_string()));
    }
    let Some(models) = value.get("modelSummaries").and_then(Value::as_array) else {
        return body;
    };
    let data: Vec<_> = models
        .iter()
        .filter(|model| {
            model
                .pointer("/modelLifecycle/status")
                .and_then(Value::as_str)
                == Some("ACTIVE")
                && model
                    .pointer("/inferenceAPIsSupported/converse/sync")
                    .and_then(Value::as_bool)
                    == Some(true)
        })
        .filter_map(model)
        .collect();
    Bytes::from(json!({ "object": "list", "data": data }).to_string())
}

fn model(value: &Value) -> Option<Value> {
    Some(json!({
        "id": value.get("modelId")?.as_str()?,
        "object": "model",
        "created": 0,
        "owned_by": value.get("providerName").and_then(Value::as_str).unwrap_or("AWS Bedrock")
    }))
}
