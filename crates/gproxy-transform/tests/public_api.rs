use bytes::Bytes;
use gproxy_transform::protocol::{self, ContentGenerationKind, StreamFraming};
use gproxy_transform::typed::stream::TypedStreamTransform;
use gproxy_transform::typed::{RequestContext, generate_content};

#[test]
fn external_consumer_can_call_a_typed_pair_without_bytes_roundtrip() {
    let request: protocol::openai::ChatCompletionRequest =
        serde_json::from_value(serde_json::json!({
            "model":"public-name",
            "messages":[{"role":"user","content":"hello"}]
        }))
        .unwrap();
    let converted = generate_content::openai_chat_to_claude_messages::request(
        request,
        RequestContext::new("claude-sonnet-4-6", false),
    )
    .unwrap();

    assert_eq!(converted.messages.len(), 1);
    assert_eq!(
        serde_json::to_value(converted.model).unwrap(),
        "claude-sonnet-4-6"
    );
}

#[test]
fn external_consumer_can_synthesize_a_strict_stream() {
    let response = Bytes::from_static(
        br#"{
        "id":"chat","object":"chat.completion","model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]
    }"#,
    );
    let frames = gproxy_transform::synthesize_response(
        ContentGenerationKind::OpenAiChat,
        response,
        StreamFraming::Sse,
    )
    .unwrap();

    assert_eq!(frames.last().unwrap().as_ref(), b"data: [DONE]\n\n");
}

#[test]
fn external_consumer_can_drive_a_typed_stream_state_machine() {
    let event: protocol::gemini::GenerateContentResponse =
        serde_json::from_value(serde_json::json!({
            "responseId":"gemini",
            "modelVersion":"gemini-test",
            "candidates":[{
                "index":0,
                "content":{"role":"model","parts":[{"text":"ok"}]},
                "finishReason":"STOP"
            }]
        }))
        .unwrap();
    let mut stream = gproxy_transform::typed::stream::openai_chat_to_gemini_generate_content::StreamTransform::default();
    let output = stream.push(event).unwrap();
    stream.finish().unwrap();

    assert_eq!(output[0].choices[0].delta.content.as_deref(), Some("ok"));
}
