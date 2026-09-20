use claude_chat::ResponseSupplement;
use gproxy_protocol::{
    claude::generate_content as cg, openai::chat, transform::generate::claude_chat,
};
use serde_json::json;

#[test]
fn claude_response_maps_text_tool_use_stop_and_usage() {
    let input: cg::GenerateContentResponseBody = serde_json::from_value(json!({
        "type":"message","id":"msg_1","model":"claude-source","role":"assistant","stop_reason":"tool_use","stop_sequence":null,
        "content":[{"type":"text","text":"answer"},{"type":"tool_use","id":"toolu_1","name":"lookup","input":{"q":"x"}}],
        "usage":{"input_tokens":11,"output_tokens":7,"cache_read_input_tokens":3}
    })).unwrap();
    let converted = claude_chat::claude_response_to_openai(
        &input,
        "gpt-target",
        &ResponseSupplement {
            created_unix_seconds: Some(1),
        },
    )
    .unwrap();
    assert_eq!(converted.value.model, "gpt-target");
    assert_eq!(
        converted.value.choices[0].finish_reason,
        chat::FinishReason::ToolCalls
    );
    assert_eq!(converted.value.usage.as_ref().unwrap().prompt_tokens, 14);
    assert_eq!(converted.value.usage.as_ref().unwrap().total_tokens, 21);
    assert!(converted.value.choices[0].message.tool_calls.is_some());
    assert!(converted.value.rest.is_empty());
}

#[test]
fn claude_response_requires_factual_created_time() {
    let input: cg::GenerateContentResponseBody = serde_json::from_value(json!({
        "type":"message","id":"msg","model":"claude","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"output_tokens":1}
    })).unwrap();
    let error =
        claude_chat::claude_response_to_openai(&input, "gpt", &ResponseSupplement::default())
            .unwrap_err();
    assert_eq!(error.context(), "response.created");
}

#[test]
fn openai_response_maps_refusal_tool_calls_usage_and_unknown_rest_is_dropped() {
    let input: chat::GenerateContentResponseBody = serde_json::from_value(json!({
        "id":"chatcmpl_1","object":"chat.completion","created":1,"model":"gpt-source",
        "choices":[{"index":0,"finish_reason":"tool_calls","logprobs":null,"message":{"role":"assistant","content":null,"refusal":"no","tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}]}}],
        "usage":{"prompt_tokens":4,"completion_tokens":3,"total_tokens":7,"prompt_tokens_details":{"cached_tokens":1}},
        "unknown":{"nested":true}
    })).unwrap();
    let converted = claude_chat::openai_response_to_claude(
        &input,
        "claude-target",
        &ResponseSupplement::default(),
    )
    .unwrap();
    assert_eq!(converted.value.stop_reason, cg::StopReason::ToolUse);
    assert_eq!(converted.value.usage.input_tokens, 3);
    let encoded = serde_json::to_value(&converted.value).unwrap();
    assert!(encoded.get("unknown").is_none());
    assert!(encoded.to_string().contains("tool_use"));
}

#[test]
fn malformed_response_arguments_are_invalid_results() {
    let input: chat::GenerateContentResponseBody = serde_json::from_value(json!({
        "id":"chatcmpl_1","object":"chat.completion","created":1,"model":"gpt",
        "choices":[{"index":0,"finish_reason":"tool_calls","logprobs":null,"message":{"role":"assistant","content":null,"refusal":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"[]"}}]}}]
    })).unwrap();
    let error =
        claude_chat::openai_response_to_claude(&input, "claude", &ResponseSupplement::default())
            .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::InvalidResult
    );
}

#[test]
fn chat_response_requires_actual_usage() {
    let input: chat::GenerateContentResponseBody = serde_json::from_value(json!({
        "id":"chat","object":"chat.completion","created":1,"model":"gpt",
        "choices":[{"index":0,"finish_reason":"stop","logprobs":null,"message":{"role":"assistant","content":"ok","refusal":null}}]
    })).unwrap();
    let error =
        claude_chat::openai_response_to_claude(&input, "claude", &ResponseSupplement::default())
            .unwrap_err();
    assert_eq!(error.context(), "response.usage");
}

#[test]
fn cache_accounting_roundtrips_and_refusal_keeps_both_texts() {
    let input: chat::GenerateContentResponseBody=serde_json::from_value(json!({
        "id":"chat","object":"chat.completion","created":123,"model":"actual",
        "choices":[{"index":0,"finish_reason":"content_filter","logprobs":null,"message":{"role":"assistant","content":"visible","refusal":"refusal"}}],
        "usage":{"prompt_tokens":10,"completion_tokens":4,"total_tokens":14,"prompt_tokens_details":{"cached_tokens":3,"cache_write_tokens":2},"completion_tokens_details":{"reasoning_tokens":1}}
    })).unwrap();
    let converted =
        claude_chat::openai_response_to_claude(&input, "actual", &ResponseSupplement::default())
            .unwrap()
            .value;
    assert_eq!(converted.usage.input_tokens, 5);
    assert_eq!(converted.usage.cache_creation_input_tokens, Some(Some(2)));
    assert_eq!(converted.usage.cache_read_input_tokens, Some(Some(3)));
    let texts: Vec<_> = converted
        .content
        .iter()
        .filter_map(|block| match block {
            cg::ResponseContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["visible", "refusal"]);
    let back = claude_chat::claude_response_to_openai(
        &converted,
        "actual",
        &ResponseSupplement {
            created_unix_seconds: Some(123),
        },
    )
    .unwrap()
    .value;
    assert_eq!(back.usage.as_ref().unwrap().prompt_tokens, 10);
    assert_eq!(back.usage.as_ref().unwrap().total_tokens, 14);
    assert_eq!(
        back.usage
            .as_ref()
            .unwrap()
            .completion_tokens_details
            .as_ref()
            .unwrap()
            .reasoning_tokens,
        Some(1)
    );
    assert_eq!(
        back.choices[0].message.refusal.as_deref(),
        Some("visiblerefusal")
    );
    assert!(back.choices[0].message.content.is_none());
}

#[test]
fn invalid_usage_overflow_breakdown_and_multiple_choices_fail() {
    let base = json!({"type":"message","id":"msg","model":"actual","role":"assistant","stop_reason":"end_turn","content":[{"type":"text","text":"ok"}],"usage":{"input_tokens":1,"output_tokens":1}});
    for usage in [
        json!({"input_tokens":-1,"output_tokens":1}),
        json!({"input_tokens":i64::MAX,"output_tokens":1}),
        json!({"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":2,"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":2}}),
    ] {
        let overflow = usage["input_tokens"] == json!(i64::MAX);
        let mut input = base.clone();
        input["usage"] = usage;
        let input = serde_json::from_value(input).unwrap();
        let result = claude_chat::claude_response_to_openai(
            &input,
            "actual",
            &ResponseSupplement {
                created_unix_seconds: Some(1),
            },
        );
        assert_eq!(result.is_err(), overflow);
    }
    let mut input:chat::GenerateContentResponseBody=serde_json::from_value(json!({"id":"chat","object":"chat.completion","created":1,"model":"actual","choices":[{"index":0,"finish_reason":"stop","logprobs":null,"message":{"role":"assistant","content":"a","refusal":null}},{"index":1,"finish_reason":"stop","logprobs":null,"message":{"role":"assistant","content":"b","refusal":null}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}})).unwrap();
    assert!(
        claude_chat::openai_response_to_claude(&input, "actual", &ResponseSupplement::default())
            .is_ok()
    );
    input.choices.truncate(1);
    input.usage.as_mut().unwrap().prompt_tokens_details = Some(
        chat::PromptTokensDetails::builder()
            .cached_tokens(2)
            .build(),
    );
    assert!(
        claude_chat::openai_response_to_claude(&input, "actual", &ResponseSupplement::default())
            .is_ok()
    );
}

#[test]
fn visible_thinking_roundtrips_without_becoming_answer_text() {
    let source:cg::GenerateContentResponseBody=serde_json::from_value(json!({"type":"message","id":"msg","role":"assistant","model":"m","content":[{"type":"thinking","thinking":"plan","signature":"source-signature"},{"type":"text","text":"answer"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":3,"output_tokens":4}})).unwrap();
    let chat = claude_chat::claude_response_to_openai(
        &source,
        "m",
        &ResponseSupplement {
            created_unix_seconds: Some(1),
        },
    )
    .unwrap()
    .value;
    assert_eq!(chat.choices[0].message.content.as_deref(), Some("answer"));
    assert_eq!(
        chat.choices[0].message.reasoning_content,
        Some(Some("plan".into()))
    );
    let restored =
        claude_chat::openai_response_to_claude(&chat, "m", &ResponseSupplement::default())
            .unwrap()
            .value;
    let cg::ResponseContentBlock::Thinking(thinking) = &restored.content[0] else {
        panic!("thinking")
    };
    assert_eq!(thinking.thinking, "plan");
    assert_eq!(
        thinking.signature, "source-signature",
        "the native signature travels in formal reasoning details"
    );
}
