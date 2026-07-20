use serde_json::{Value, json};

use super::*;

const MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

fn count_breakpoints(value: &Value) -> usize {
    match value {
        Value::Array(values) => values.iter().map(count_breakpoints).sum(),
        Value::Object(object) => {
            usize::from(object.contains_key("prompt_cache_breakpoint"))
                + object.values().map(count_breakpoints).sum::<usize>()
        }
        _ => 0,
    }
}

#[test]
fn chat_magic_converts_string_content_and_stamps_breakpoint() {
    let mut body = json!({
        "model": "gpt-5.6",
        "messages": [{"role": "system", "content": format!("stable {MAGIC}")}]
    });
    apply_magic_string_cache_breakpoints(&mut body, ContentGenerationKind::OpenAiChatCompletions);

    assert_eq!(body["messages"][0]["content"][0]["text"], "stable ");
    assert_eq!(
        body["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    serde_json::from_value::<crate::protocol::openai::generate_content::ChatCompletionRequest>(
        body,
    )
    .unwrap();
}

#[test]
fn responses_instruction_magic_uses_prefix_anchor() {
    let mut body = json!({"instructions": format!("stable {MAGIC}"), "input": "hello"});
    apply_magic_string_cache_breakpoints(&mut body, ContentGenerationKind::OpenAiResponses);

    assert_eq!(body["instructions"], "stable ");
    assert_eq!(body["input"][0]["role"], "developer");
    assert_eq!(
        body["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(body["input"][1]["content"][0]["text"], "hello");
    serde_json::from_value::<crate::protocol::openai::generate_content::ResponseCreateRequest>(
        body,
    )
    .unwrap();
}

#[test]
fn magic_caps_new_markers_without_counting_prior_turns() {
    let marked = |text: &str| {
        json!({
            "type": "input_text",
            "text": text,
            "prompt_cache_breakpoint": {"mode": "explicit"}
        })
    };
    let mut body = json!({"input": [{
        "role": "user",
        "content": [
            marked("a"), marked("b"), marked("c"),
            {"type": "input_text", "text": format!("d {MAGIC}")},
            {"type": "input_text", "text": format!("e {MAGIC}")}
        ]
    }]});
    apply_magic_string_cache_breakpoints(&mut body, ContentGenerationKind::OpenAiResponses);

    assert_eq!(count_breakpoints(&body), 5);
    assert!(!body.to_string().contains(MAGIC));
    assert!(body["input"][0]["content"][4]["prompt_cache_breakpoint"].is_object());
}

#[test]
fn manual_message_flattens_supported_parts_across_chat_messages() {
    let mut body = json!({"messages": [
        {"role": "user", "content": [
            {"type": "text", "text": "first"},
            {"type": "custom", "value": "unsupported"}
        ]},
        {"role": "assistant", "content": [
            {"type": "text", "text": "second"},
            {"type": "text", "text": "   "}
        ]}
    ]});

    apply_manual_cache_breakpoint(
        &mut body,
        ContentGenerationKind::OpenAiChatCompletions,
        "message",
        Some(-2),
        None,
    )
    .unwrap();

    assert!(body["messages"][0]["content"][0]["prompt_cache_breakpoint"].is_object());
    assert!(
        body["messages"][0]["content"][1]
            .get("prompt_cache_breakpoint")
            .is_none()
    );
}

#[test]
fn manual_responses_message_sets_request_ttl() {
    let mut body = json!({"input": [{"role": "user", "content": "hello"}]});
    apply_manual_cache_breakpoint(
        &mut body,
        ContentGenerationKind::OpenAiResponses,
        "message",
        None,
        Some("30m"),
    )
    .unwrap();

    assert_eq!(
        body["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(body["prompt_cache_options"]["ttl"], "30m");
}
