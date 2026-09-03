use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::typed::RequestContext;
use crate::typed::stream::TypedStreamTransform;
use crate::{BufferedResponse, ResponseCollector, TransformError, request, response};

fn key(kind: Kind) -> OperationKey {
    OperationKey::content(Operation::GenerateContent, kind)
}

fn request_parity<S, T>(
    source: Kind,
    target: Kind,
    input: Value,
    typed: fn(S, RequestContext<'_>) -> Result<T, TransformError>,
) where
    S: DeserializeOwned,
    T: Serialize,
{
    let typed = typed(
        serde_json::from_value(input.clone()).unwrap(),
        RequestContext::new("upstream-model", false),
    )
    .unwrap();
    let bytes = request(
        key(source),
        key(target),
        Bytes::from(serde_json::to_vec(&input).unwrap()),
        "upstream-model",
        false,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(typed).unwrap(),
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        "{source:?} -> {target:?}",
    );
}

fn response_parity<T, S>(
    source: Kind,
    target: Kind,
    input: Value,
    typed: fn(T) -> Result<S, TransformError>,
) where
    T: DeserializeOwned,
    S: Serialize,
{
    let typed = typed(serde_json::from_value(input.clone()).unwrap()).unwrap();
    let bytes = response(
        key(source),
        key(target),
        Bytes::from(serde_json::to_vec(&input).unwrap()),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(typed).unwrap(),
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        "{target:?} -> {source:?} response",
    );
}

#[test]
fn every_content_request_pair_uses_the_typed_core() {
    use crate::typed::generate_content as typed;

    let chat = json!({"model":"route","messages":[{"role":"user","content":"hi"}]});
    let responses = json!({"model":"route","input":"hi"});
    let claude = json!({
        "model":"route","max_tokens":32,
        "messages":[{"role":"user","content":"hi"}]
    });
    let gemini = json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]});

    request_parity(
        Kind::OpenAiChat,
        Kind::ClaudeMessages,
        chat.clone(),
        typed::openai_chat_to_claude_messages::request,
    );
    request_parity(
        Kind::ClaudeMessages,
        Kind::OpenAiChat,
        claude.clone(),
        typed::claude_messages_to_openai_chat::request,
    );
    request_parity(
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        responses.clone(),
        typed::openai_responses_to_claude_messages::request,
    );
    request_parity(
        Kind::ClaudeMessages,
        Kind::OpenAiResponses,
        claude.clone(),
        typed::claude_messages_to_openai_responses::request,
    );
    request_parity(
        Kind::OpenAiChat,
        Kind::GeminiGenerateContent,
        chat.clone(),
        typed::openai_chat_to_gemini_generate_content::request,
    );
    request_parity(
        Kind::GeminiGenerateContent,
        Kind::OpenAiChat,
        gemini.clone(),
        typed::gemini_generate_content_to_openai_chat::request,
    );
    request_parity(
        Kind::OpenAiResponses,
        Kind::GeminiGenerateContent,
        responses.clone(),
        typed::openai_responses_to_gemini_generate_content::request,
    );
    request_parity(
        Kind::GeminiGenerateContent,
        Kind::OpenAiResponses,
        gemini.clone(),
        typed::gemini_generate_content_to_openai_responses::request,
    );
    request_parity(
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
        claude.clone(),
        typed::claude_messages_to_gemini_generate_content::request,
    );
    request_parity(
        Kind::GeminiGenerateContent,
        Kind::ClaudeMessages,
        gemini,
        typed::gemini_generate_content_to_claude_messages::request,
    );
    request_parity(
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        chat,
        typed::openai_chat_to_openai_responses::request,
    );
    request_parity(
        Kind::OpenAiResponses,
        Kind::OpenAiChat,
        responses,
        typed::openai_responses_to_openai_chat::request,
    );
}

#[test]
fn every_content_response_pair_uses_the_typed_core() {
    use crate::typed::generate_content as typed;

    let chat = json!({
        "id":"chat","object":"chat.completion","model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]
    });
    let responses = json!({
        "id":"resp","object":"response","status":"completed","model":"m",
        "output":[{"type":"message","id":"msg","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"ok","annotations":[]}]}]
    });
    let claude = json!({
        "id":"msg","type":"message","role":"assistant","model":"m",
        "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
        "stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}
    });
    let gemini = json!({
        "responseId":"gemini","modelVersion":"m",
        "candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}]
    });

    response_parity(
        Kind::OpenAiChat,
        Kind::ClaudeMessages,
        claude.clone(),
        typed::openai_chat_to_claude_messages::response,
    );
    response_parity(
        Kind::ClaudeMessages,
        Kind::OpenAiChat,
        chat.clone(),
        typed::claude_messages_to_openai_chat::response,
    );
    response_parity(
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        claude.clone(),
        typed::openai_responses_to_claude_messages::response,
    );
    response_parity(
        Kind::ClaudeMessages,
        Kind::OpenAiResponses,
        responses.clone(),
        typed::claude_messages_to_openai_responses::response,
    );
    response_parity(
        Kind::OpenAiChat,
        Kind::GeminiGenerateContent,
        gemini.clone(),
        typed::openai_chat_to_gemini_generate_content::response,
    );
    response_parity(
        Kind::GeminiGenerateContent,
        Kind::OpenAiChat,
        chat.clone(),
        typed::gemini_generate_content_to_openai_chat::response,
    );
    response_parity(
        Kind::OpenAiResponses,
        Kind::GeminiGenerateContent,
        gemini.clone(),
        typed::openai_responses_to_gemini_generate_content::response,
    );
    response_parity(
        Kind::GeminiGenerateContent,
        Kind::OpenAiResponses,
        responses.clone(),
        typed::gemini_generate_content_to_openai_responses::response,
    );
    response_parity(
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
        gemini,
        typed::claude_messages_to_gemini_generate_content::response,
    );
    response_parity(
        Kind::GeminiGenerateContent,
        Kind::ClaudeMessages,
        claude,
        typed::gemini_generate_content_to_claude_messages::response,
    );
    response_parity(
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        responses,
        typed::openai_chat_to_openai_responses::response,
    );
    response_parity(
        Kind::OpenAiResponses,
        Kind::OpenAiChat,
        chat,
        typed::openai_responses_to_openai_chat::response,
    );
}

fn assert_stream<T: TypedStreamTransform + Default>() {}

#[test]
fn every_content_pair_exposes_a_typed_stream_state_machine() {
    use crate::typed::stream::*;

    assert_stream::<openai_chat_to_claude_messages::StreamTransform>();
    assert_stream::<claude_messages_to_openai_chat::StreamTransform>();
    assert_stream::<openai_responses_to_claude_messages::StreamTransform>();
    assert_stream::<claude_messages_to_openai_responses::StreamTransform>();
    assert_stream::<openai_chat_to_gemini_generate_content::StreamTransform>();
    assert_stream::<gemini_generate_content_to_openai_chat::StreamTransform>();
    assert_stream::<openai_responses_to_gemini_generate_content::StreamTransform>();
    assert_stream::<gemini_generate_content_to_openai_responses::StreamTransform>();
    assert_stream::<claude_messages_to_gemini_generate_content::StreamTransform>();
    assert_stream::<gemini_generate_content_to_claude_messages::StreamTransform>();
    assert_stream::<openai_chat_to_openai_responses::StreamTransform>();
    assert_stream::<openai_responses_to_openai_chat::StreamTransform>();
}

fn synthesize_and_collect(kind: Kind, response: Value) -> BufferedResponse {
    let frames = crate::synthesize_response(
        kind,
        Bytes::from(serde_json::to_vec(&response).unwrap()),
        gproxy_protocol::StreamFraming::Sse,
    )
    .unwrap();
    let mut collector = ResponseCollector::new(kind).unwrap();
    for frame in frames {
        collector.push(frame).unwrap();
    }
    collector.finish().unwrap()
}

#[test]
fn buffered_responses_synthesize_strict_streams_for_every_content_protocol() {
    let chat = json!({
        "id":"chat","object":"chat.completion","model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]
    });
    let responses = json!({
        "id":"resp","object":"response","status":"completed","model":"m",
        "output":[{"type":"message","id":"msg","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"ok","annotations":[]}]}]
    });
    let claude = json!({
        "id":"msg","type":"message","role":"assistant","model":"m",
        "content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
        "stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}
    });
    let gemini = json!({
        "responseId":"gemini","modelVersion":"m",
        "candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}]
    });

    let BufferedResponse::OpenAiChat(chat) = synthesize_and_collect(Kind::OpenAiChat, chat) else {
        panic!("chat collector returned another protocol")
    };
    assert_eq!(chat.choices[0].message.content.as_deref(), Some("ok"));

    let BufferedResponse::OpenAiResponses(responses) =
        synthesize_and_collect(Kind::OpenAiResponses, responses)
    else {
        panic!("Responses collector returned another protocol")
    };
    assert_eq!(responses.id, "resp");
    assert_eq!(responses.output.len(), 1);

    let BufferedResponse::Claude(claude) = synthesize_and_collect(Kind::ClaudeMessages, claude)
    else {
        panic!("Claude collector returned another protocol")
    };
    assert_eq!(claude.id, "msg");
    assert_eq!(claude.content.len(), 1);

    let BufferedResponse::Gemini(gemini) =
        synthesize_and_collect(Kind::GeminiGenerateContent, gemini)
    else {
        panic!("Gemini collector returned another protocol")
    };
    assert_eq!(gemini.response_id.as_deref(), Some("gemini"));
}

#[test]
fn synthetic_stream_framing_is_protocol_specific() {
    let gemini = Bytes::from_static(
        br#"{
        "responseId":"g","modelVersion":"m",
        "candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}]
    }"#,
    );
    let array = crate::synthesize_response(
        Kind::GeminiGenerateContent,
        gemini,
        gproxy_protocol::StreamFraming::JsonArray,
    )
    .unwrap();
    assert_eq!(array.len(), 1);
    assert!(array[0].starts_with(b"["));

    let chat = Bytes::from_static(
        br#"{
        "id":"c","object":"chat.completion","model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]
    }"#,
    );
    assert!(
        crate::synthesize_response(
            Kind::OpenAiChat,
            chat,
            gproxy_protocol::StreamFraming::JsonArray,
        )
        .is_err()
    );
}
