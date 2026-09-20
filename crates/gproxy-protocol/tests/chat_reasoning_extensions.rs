use gproxy_protocol::{
    Dialect,
    transform::{
        generate::stream::chat::{ChatStreamCollector, ChatStreamLimits, synthesize_chat_stream},
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::openai::chat as c,
};
use serde_json::json;

#[test]
fn deepseek_and_openrouter_reasoning_roundtrip_and_collect() {
    let mut collector = ChatStreamCollector::new(
        IdentityFlow::new(gproxy_protocol::transform::identity::IdNamespace::with_bytes([91; 16])),
        TargetIdPolicy::new(Dialect::OpenAiChat),
    );
    for (text, detail) in [("think ", "first "), ("more", "second")] {
        let value = json!({"id":"chat1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":text,"reasoning":text,"reasoning_details":[{"type":"reasoning.text","index":0,"format":"unknown","text":detail}]}}]});
        let chunk: c::stream::ChatCompletionChunk = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&chunk).unwrap(), value);
        collector.push(chunk).unwrap();
    }
    collector.push(serde_json::from_value(json!({"id":"chat1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"content":"answer"},"finish_reason":"stop"}]})).unwrap()).unwrap();
    collector.push_done().unwrap();
    let response = collector.finish().unwrap().value;
    let message = &response.choices[0].message;
    assert_eq!(message.reasoning_content, Some(Some("think more".into())));
    assert_eq!(message.reasoning, Some(Some("think more".into())));
    assert_eq!(
        message
            .reasoning_details
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()[0]
            .text,
        Some(Some("first second".into()))
    );
    assert_eq!(
        c::visible_reasoning(
            &message.reasoning_content,
            &message.reasoning,
            &message.reasoning_details
        ),
        Some("think more".into())
    );
    let chunks = synthesize_chat_stream(response, ChatStreamLimits::default())
        .unwrap()
        .value;
    assert_eq!(
        chunks[0].choices[0].delta.reasoning_content,
        Some(Some("think more".into()))
    );
}
#[test]
fn encrypted_details_are_not_visible_text_and_assistant_replay_keeps_them() {
    let value = json!({"role":"assistant","content":null,"reasoning_content":"","reasoning_details":[{"type":"reasoning.encrypted","data":"opaque","id":"r1","format":"openai-responses-v1"}]});
    let message: c::ChatMessage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&message).unwrap(), value);
    let c::ChatMessage::Assistant(message) = message else {
        panic!("assistant")
    };
    assert_eq!(
        c::visible_reasoning(
            &message.reasoning_content,
            &message.reasoning,
            &message.reasoning_details
        ),
        Some("".into())
    );
}

#[test]
fn compatible_reasoning_only_response_can_omit_openai_nullable_extras() {
    let body:c::GenerateContentResponseBody=serde_json::from_value(json!({"id":"r","object":"chat.completion","created":1,"model":"deepseek-reasoner","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","reasoning_content":"analysis"}}]})).unwrap();
    assert_eq!(
        body.choices[0].message.reasoning_content,
        Some(Some("analysis".into()))
    );
    assert!(body.choices[0].message.content.is_none());
    assert!(body.choices[0].message.refusal.is_none());
    assert!(body.choices[0].logprobs.is_none());
}
