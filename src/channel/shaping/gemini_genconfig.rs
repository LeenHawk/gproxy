//! Gemini `generationConfig` hygiene.

use serde_json::Value;

/// Remove `generationConfig` fields that the Gemini CLI / Antigravity upstreams
/// reject: `maxOutputTokens`, `logprobs`, `responseLogprobs` (plus the
/// snake_case `max_output_tokens` / `response_logprobs` aliases if present).
///
/// No-op when there is no object `generationConfig`. Safe to call on every
/// operation.
pub fn strip(body: &mut Value) {
    let Some(config) = body
        .get_mut("generationConfig")
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    config.remove("maxOutputTokens");
    config.remove("max_output_tokens");
    config.remove("logprobs");
    config.remove("responseLogprobs");
    config.remove("response_logprobs");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_unsupported_fields() {
        let mut body = json!({
            "generationConfig": {
                "maxOutputTokens": 1024,
                "max_output_tokens": 1024,
                "logprobs": 3,
                "responseLogprobs": true,
                "response_logprobs": true,
                "temperature": 0.5
            }
        });
        strip(&mut body);
        let cfg = body["generationConfig"].as_object().unwrap();
        assert!(!cfg.contains_key("maxOutputTokens"));
        assert!(!cfg.contains_key("max_output_tokens"));
        assert!(!cfg.contains_key("logprobs"));
        assert!(!cfg.contains_key("responseLogprobs"));
        assert!(!cfg.contains_key("response_logprobs"));
        // unrelated fields preserved
        assert_eq!(cfg.get("temperature"), Some(&json!(0.5)));
    }

    #[test]
    fn noop_without_generation_config() {
        let mut body = json!({"model": "gemini-x"});
        let before = body.clone();
        strip(&mut body);
        assert_eq!(body, before);
    }
}
