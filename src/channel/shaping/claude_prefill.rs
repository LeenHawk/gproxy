//! Trailing assistant-prefill hygiene for Claude request bodies.

use serde_json::Value;

/// Claude models that remain accessible and support assistant prefill.
///
/// Retired models such as Claude 1/2/Instant, other Claude 3 variants, and
/// Opus/Sonnet 4.0 are omitted because upstream requests to them already fail.
/// Claude 3 Opus is officially retired but remains listed because some accounts
/// retain access. Models not in this list, including unknown and future models,
/// are treated as prefill-intolerant because new models starting with Opus 4.6
/// reject prefill. Substring matching covers provider namespaces such as
/// `anthropic/` and `anthropic.` as well as dated model variants.
const PREFILL_TOLERANT_MODELS: &[&str] = &[
    "claude-3-opus",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
];

/// Coerce a trailing assistant prefill to a user turn for newer Claude models.
///
/// Both Claude Messages and Anthropic's OpenAI-compatible request bodies use a
/// `messages` array of objects with a `role` field, so this shaper supports
/// both forms. Legacy models in [`PREFILL_TOLERANT_MODELS`] retain prefill.
/// Trailing OpenAI `tool` results and existing `user` turns are also preserved.
///
/// Idempotent and a no-op for non-object bodies, missing or non-Claude models,
/// and missing or empty message arrays.
pub fn coerce_trailing_prefill(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(model) = root
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
    else {
        return;
    };
    if !model.contains("claude")
        || PREFILL_TOLERANT_MODELS
            .iter()
            .any(|model_id| model.contains(model_id))
    {
        return;
    }

    let Some(last) = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .and_then(|messages| messages.last_mut())
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if matches!(
        last.get("role").and_then(Value::as_str),
        Some("tool" | "user")
    ) {
        return;
    }
    last.insert("role".into(), Value::String("user".into()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerces_new_model_prefill_and_preserves_other_fields() {
        let mut body = json!({
            "model": "claude-opus-4-8",
            "messages": [
                {"role": "user", "content": "question"},
                {"role": "assistant", "content": "prefix", "metadata": {"kept": true}}
            ],
            "max_tokens": 64
        });

        coerce_trailing_prefill(&mut body);
        coerce_trailing_prefill(&mut body);

        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "prefix");
        assert_eq!(body["messages"][1]["metadata"], json!({"kept": true}));
        assert_eq!(body["max_tokens"], 64);
    }

    #[test]
    fn preserves_prefill_for_legacy_models_and_namespaces() {
        for model in ["claude-opus-4-5", "anthropic/claude-3-opus-20240229"] {
            let mut body = json!({
                "model": model,
                "messages": [{"role": "assistant", "content": "prefix"}]
            });
            coerce_trailing_prefill(&mut body);
            assert_eq!(body["messages"][0]["role"], "assistant", "{model}");
        }
    }

    #[test]
    fn handles_openai_assistant_and_tool_turns() {
        let mut assistant = json!({
            "model": "claude-fable-5",
            "messages": [{"role": "assistant", "content": "prefix"}]
        });
        let mut tool = json!({
            "model": "claude-fable-5",
            "messages": [{"role": "tool", "tool_call_id": "call_1", "content": "result"}]
        });

        coerce_trailing_prefill(&mut assistant);
        coerce_trailing_prefill(&mut tool);

        assert_eq!(assistant["messages"][0]["role"], "user");
        assert_eq!(tool["messages"][0]["role"], "tool");
    }

    #[test]
    fn preserves_non_claude_models() {
        let mut body = json!({
            "model": "gpt-4o",
            "messages": [{"role": "assistant", "content": "prefix"}]
        });
        let before = body.clone();
        coerce_trailing_prefill(&mut body);
        assert_eq!(body, before);
    }

    #[test]
    fn edge_cases_are_noops() {
        for mut body in [
            json!({"model": "claude-opus-4-8", "messages": [{"role": "user"}]}),
            json!({"model": "claude-opus-4-8", "messages": []}),
            json!(["not", "an", "object"]),
        ] {
            let before = body.clone();
            coerce_trailing_prefill(&mut body);
            assert_eq!(body, before);
        }
    }
}
