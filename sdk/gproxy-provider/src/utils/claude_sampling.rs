use serde_json::Value;

/// Models that are known to tolerate sampling parameters. For these models
/// we apply the light rule only: strip `top_p` when `temperature` is
/// present. Any model **not** in this list gets the conservative default
/// of stripping all three (`temperature`, `top_p`, `top_k`).
///
/// Entries are used as prefixes so that dated variants (e.g.
/// `claude-sonnet-4-6-20260101`) also match. The 4.0-generation models
/// need two prefixes each: the `-0` alias and the `-20` dated form
/// (e.g. `claude-opus-4-20250514`), because the dated ID drops the `-0`.
const SAMPLING_TOLERANT_MODELS: &[&str] = &[
    // Current
    "claude-sonnet-4-6",
    "claude-haiku-4-5",
    // Legacy 4.5
    "claude-sonnet-4-5",
    "claude-opus-4-5",
    // Legacy 4.1
    "claude-opus-4-1",
    // Legacy 4.0 — alias uses `-0`, dated ID uses bare `-4-YYYYMMDD`
    "claude-sonnet-4-0",
    "claude-sonnet-4-20", // dated: claude-sonnet-4-20250514
    "claude-opus-4-0",
    "claude-opus-4-20", // dated: claude-opus-4-20250514
    // Legacy 3.x
    "claude-3-haiku",
];

/// Normalize sampling parameters on a Claude request body.
///
/// Used by the `claude` (anthropic direct) and `claudecode` channels before
/// forwarding to upstream.
///
/// Behavior depends on the model in the request:
///
/// - **Models in [`SAMPLING_TOLERANT_MODELS`]**: if `temperature` is
///   present, strip `top_p` only (the two interact poorly on Claude).
///   `temperature` and `top_k` are left untouched.
/// - **All other models** (including unknown / future models): strip
///   `temperature`, `top_p`, and `top_k` unconditionally — the
///   conservative default.
///
/// Idempotent and a no-op on non-object bodies (model_list / model_get /
/// empty bodies). Safe to call on every operation.
pub fn strip_sampling_params(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    let tolerant = map
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|m| SAMPLING_TOLERANT_MODELS.iter().any(|&prefix| m.starts_with(prefix)));

    if tolerant {
        if map.contains_key("temperature") {
            map.remove("top_p");
        }
    } else {
        map.remove("temperature");
        map.remove("top_p");
        map.remove("top_k");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Full-strip model (opus-4-6) ──────────────────────────────────

    #[test]
    fn opus_46_strips_all_three() {
        let mut body = json!({
            "model": "claude-opus-4-6",
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
        assert_eq!(map.get("max_tokens").and_then(Value::as_u64), Some(1024));
    }

    #[test]
    fn opus_46_variant_id_strips_all_three() {
        let mut body = json!({
            "model": "claude-opus-4-6-20260205",
            "messages": [],
            "temperature": 0.5,
            "top_p": 0.8,
        });

        strip_sampling_params(&mut body);

        let map = body.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
    }

    // ── Other models: temperature present → strip top_p only ─────────

    #[test]
    fn other_model_with_temperature_strips_top_p_only() {
        let mut body = json!({
            "model": "claude-sonnet-4-6",
            "messages": [],
            "temperature": 0.7,
            "top_p": 0.9,
            "top_k": 40,
        });

        strip_sampling_params(&mut body);

        let map = body.as_object().unwrap();
        // temperature kept
        assert!(map.contains_key("temperature"));
        // top_p stripped
        assert!(!map.contains_key("top_p"));
        // top_k kept
        assert!(map.contains_key("top_k"));
    }

    #[test]
    fn other_model_without_temperature_keeps_everything() {
        let mut body = json!({
            "model": "claude-sonnet-4-5",
            "messages": [],
            "top_p": 0.9,
            "top_k": 40,
        });
        let before = body.clone();
        strip_sampling_params(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn other_model_no_sampling_params_noop() {
        let mut body = json!({
            "model": "claude-haiku-4-5",
            "messages": [],
        });
        let before = body.clone();
        strip_sampling_params(&mut body);
        assert_eq!(body, before);
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn noop_on_non_object_body() {
        let mut body = json!([1, 2, 3]);
        let before = body.clone();
        strip_sampling_params(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn no_model_field_strips_all_three() {
        let mut body = json!({
            "messages": [],
            "temperature": 1.0,
            "top_p": 0.5,
            "top_k": 20,
        });
        strip_sampling_params(&mut body);
        let map = body.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
        assert!(!map.contains_key("top_k"));
    }

    #[test]
    fn unknown_model_strips_all_three() {
        let mut body = json!({
            "model": "claude-future-99",
            "messages": [],
            "temperature": 1.0,
            "top_p": 0.5,
            "top_k": 20,
        });
        strip_sampling_params(&mut body);
        let map = body.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
        assert!(!map.contains_key("top_k"));
    }
}
