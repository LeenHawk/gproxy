use serde_json::Value;

/// Strip sampling parameters (`temperature`, `top_p`, `top_k`) from the top
/// level of a Claude request body.
///
/// Used by the `claude` (anthropic direct) and `claudecode` channels before
/// forwarding to upstream. Anthropic's newer models are sensitive to these
/// fields — some reject non-default `temperature`, others reject the
/// presence of `top_p`/`top_k` entirely, and clients routinely send values
/// that were tuned for a different provider. Rather than thread
/// model-specific allowlists, we drop them across the board.
///
/// Idempotent and a no-op on non-object bodies (model_list / model_get /
/// empty bodies). Safe to call on every operation.
pub fn strip_sampling_params(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };
    map.remove("temperature");
    map.remove("top_p");
    map.remove("top_k");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_all_three_sampling_params_when_present() {
        let mut body = json!({
            "model": "claude-sonnet-4-5",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 0.7,
            "top_p": 0.9,
            "top_k": 40,
            "max_tokens": 1024,
        });

        strip_sampling_params(&mut body);

        let map = body.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
        assert!(!map.contains_key("top_k"));
        // Non-sampling fields are untouched.
        assert_eq!(
            map.get("model").and_then(Value::as_str),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(map.get("max_tokens").and_then(Value::as_u64), Some(1024));
        assert!(map.get("messages").is_some());
    }

    #[test]
    fn noop_when_fields_missing() {
        let mut body = json!({
            "model": "claude-sonnet-4-5",
            "messages": [],
        });
        let before = body.clone();
        strip_sampling_params(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn noop_on_non_object_body() {
        let mut body = json!([1, 2, 3]);
        let before = body.clone();
        strip_sampling_params(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn strips_partial_subset() {
        let mut body = json!({
            "messages": [],
            "top_k": 20,
        });
        strip_sampling_params(&mut body);
        assert!(!body.as_object().unwrap().contains_key("top_k"));
    }
}
