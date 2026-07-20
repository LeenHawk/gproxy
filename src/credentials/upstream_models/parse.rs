use serde::Serialize;
use serde_json::Value;

use crate::protocol::Provider;

/// One model offered by the upstream.
#[derive(Debug, Clone, Serialize)]
pub struct UpstreamModel {
    pub id: String,
    pub display_name: Option<String>,
}

/// Parse an upstream native model-list response into `(id, display_name)` rows.
/// openai/claude → `data[]` (`id`); gemini → `models[]` (`name`, `models/` stripped).
pub(super) fn parse_models(family: Provider, body: &[u8]) -> Vec<UpstreamModel> {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let key = match family {
        Provider::Gemini => "models",
        _ => "data",
    };
    let Some(arr) = v.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            let id = match family {
                Provider::Gemini => m
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|s| s.strip_prefix("models/").unwrap_or(s).to_owned()),
                _ => m.get("id").and_then(Value::as_str).map(str::to_owned),
            }?;
            let display_name = match family {
                Provider::Gemini => m.get("displayName"),
                Provider::Claude => m.get("display_name"),
                Provider::OpenAi => None,
            }
            .and_then(Value::as_str)
            .map(str::to_owned);
            Some(UpstreamModel { id, display_name })
        })
        .collect()
}
