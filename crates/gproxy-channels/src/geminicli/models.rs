use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("quota response JSON: {error}")))?;
    let Value::Object(output) = value else {
        return Err(ChannelError::Observe(
            "quota response is not an object".into(),
        ));
    };
    let mut ids = BTreeSet::new();
    if let Some(buckets) = output.get("buckets").and_then(Value::as_array) {
        for bucket in buckets {
            if bucket.get("tokenType").and_then(Value::as_str) != Some("REQUESTS") {
                continue;
            }
            if let Some(id) = bucket
                .get("modelId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
            {
                ids.insert(id.trim_start_matches("models/").to_owned());
            }
        }
    }
    let models = ids
        .into_iter()
        .map(|id| {
            let name = format!("models/{id}");
            json!({
                "name":name,
                "baseModelId":id.clone(),
                "supportedGenerationMethods":["generateContent","streamGenerateContent","countTokens"]
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({"models":models}))
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
