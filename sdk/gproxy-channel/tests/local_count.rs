use gproxy_channel::channel::Channel;
use gproxy_channel::channels::{
    deepseek::DeepSeekChannel, groq::GroqChannel, openrouter::OpenRouterChannel,
};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use serde_json::Value;

#[test]
fn deepseek_local_count_supports_openai_requests() {
    let body = serde_json::json!({
        "model": "gpt-5.4",
        "input": "hello world"
    });
    let bytes = DeepSeekChannel
        .handle_local(
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
            &serde_json::to_vec(&body).expect("serialize"),
        )
        .expect("local route")
        .expect("count succeeds");

    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        json.get("object").and_then(Value::as_str),
        Some("response.input_tokens")
    );
    assert!(
        json.get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn groq_local_count_supports_claude_requests() {
    let body = serde_json::json!({
        "messages": [
            { "role": "user", "content": "hello local count" }
        ],
        "model": "claude-sonnet-4-5"
    });
    let bytes = GroqChannel
        .handle_local(
            OperationFamily::CountToken,
            ProtocolKind::Claude,
            &serde_json::to_vec(&body).expect("serialize"),
        )
        .expect("local route")
        .expect("count succeeds");

    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert!(
        json.get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn openrouter_local_count_supports_gemini_requests() {
    let body = serde_json::json!({
        "generateContentRequest": {
            "model": "models/gemini-2.5-flash",
            "contents": [
                {
                    "role": "user",
                    "parts": [{ "text": "hello local gemini count" }]
                }
            ]
        }
    });
    let bytes = OpenRouterChannel
        .handle_local(
            OperationFamily::CountToken,
            ProtocolKind::Gemini,
            &serde_json::to_vec(&body).expect("serialize"),
        )
        .expect("local route")
        .expect("count succeeds");

    let json: Value = serde_json::from_slice(&bytes).expect("json");
    assert!(json.get("totalTokens").and_then(Value::as_u64).unwrap_or(0) > 0);
}
