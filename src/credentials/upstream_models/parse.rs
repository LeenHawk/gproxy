use serde::Serialize;
use serde_json::Value;

use crate::protocol::Provider;

/// One model offered by the upstream.
#[derive(Debug, Clone, Serialize)]
pub struct UpstreamModel {
    pub id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
}

/// Parse an upstream native model-list response into model metadata rows.
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
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            }
            .and_then(Value::as_str)
            .map(str::to_owned);
            let int = |key: &str| m.get(key).and_then(Value::as_i64).filter(|v| *v > 0);
            let meta_int = |key: &str| {
                m.get("meta")
                    .and_then(Value::as_object)
                    .and_then(|meta| meta.get(key))
                    .and_then(Value::as_i64)
                    .filter(|v| *v > 0)
            };
            let (context_window, max_input_tokens, max_output_tokens) = match family {
                Provider::OpenAi => (
                    int("context_length")
                        .or_else(|| int("context_window"))
                        .or_else(|| int("max_context_length"))
                        .or_else(|| int("max_model_len"))
                        .or_else(|| int("n_ctx"))
                        .or_else(|| meta_int("n_ctx")),
                    None,
                    int("max_completion_tokens").or_else(|| int("max_output_tokens")),
                ),
                Provider::Claude => (None, int("max_input_tokens"), int("max_tokens")),
                Provider::Gemini => (None, int("inputTokenLimit"), int("outputTokenLimit")),
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            Some(UpstreamModel {
                id,
                display_name,
                context_window,
                max_input_tokens,
                max_output_tokens,
            })
        })
        .collect()
}
