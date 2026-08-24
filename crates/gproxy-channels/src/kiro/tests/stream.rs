use bytes::Bytes;
use gproxy_channel_api::{Channel, StreamCtx, StreamEnd};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming};
use serde_json::{Value, json};

use super::super::KiroChannel;
use super::support::{append, event, prepare};

#[test]
fn maps_responses_request_and_fragmented_smithy_stream_without_false_terminal() {
    let key = OperationKey::content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let secret = json!({"access_token":"access","profile_arn":"arn:profile"});
    let body = Bytes::from(
        json!({
            "model":"route","instructions":"follow rules","max_output_tokens":64,
            "input":[{"type":"message","role":"user","content":[
                {"type":"input_text","text":"hello"},
                {"type":"input_image","image_url":"data:image/png;base64,AAEC"}
            ]}],
            "tools":[{"type":"function","name":"get_weather","description":"weather",
                "parameters":{"type":"object","properties":{"city":{"type":"string"}},
                    "required":[],"additionalProperties":false}}]
        })
        .to_string(),
    );
    let prepared = prepare(key, "claude-sonnet-4-6", &body, &secret, &json!({}));
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    let state = &shaped["conversationState"];
    assert_eq!(state["history"].as_array().unwrap().len(), 2);
    let current = &state["currentMessage"]["userInputMessage"];
    assert_eq!(current["content"], "hello");
    assert_eq!(current["modelId"], "claude-sonnet-4.6");
    assert_eq!(current["images"][0]["format"], "png");
    assert_eq!(
        current["userInputMessageContext"]["tools"][0]["toolSpecification"]["name"],
        "getWeather"
    );
    assert!(
        current["userInputMessageContext"]["tools"][0]["toolSpecification"]["inputSchema"]["json"]
            .get("additionalProperties")
            .is_none()
    );
    assert_eq!(shaped["inferenceConfig"]["maxTokens"], 64);

    let mut decoder = KiroChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: prepared.request.body(),
            response_headers: &http::HeaderMap::new(),
        })
        .unwrap();
    let mut wire = Vec::new();
    for frame in [
        event("assistantResponseEvent", br#"{"content":"he"}"#),
        event("assistantResponseEvent", br#"{"content":"hello%20world"}"#),
        event("reasoningContentEvent", br#"{"text":"think"}"#),
        event(
            "toolUseEvent",
            br#"{"toolUseId":"call-1","name":"tool","input":"{}","stop":true}"#,
        ),
        event(
            "metadataEvent",
            br#"{"tokenUsage":{"inputTokens":10,"outputTokens":4,"cacheReadInputTokens":3}}"#,
        ),
    ] {
        wire.extend_from_slice(&frame);
    }
    let mut output = Vec::new();
    for chunk in wire.chunks(31) {
        append(
            &mut output,
            decoder.push(Bytes::copy_from_slice(chunk)).unwrap(),
        );
    }
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    append(&mut output, tail.frames);
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("response.created"));
    assert!(text.contains(r#""delta":"he""#));
    assert!(text.contains(r#""delta":"llo world""#));
    assert!(text.contains("response.function_call_arguments.done"));
    assert!(text.contains("response.completed"));
    let usage = tail.usage.unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cached_input_tokens
        ),
        (10, 4, 3)
    );
    assert!(!text.contains("created_at"));

    let mut interrupted = KiroChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: prepared.request.body(),
            response_headers: &http::HeaderMap::new(),
        })
        .unwrap();
    let frames = interrupted
        .push(event("assistantResponseEvent", br#"{"content":"partial"}"#))
        .unwrap();
    let mut partial = Vec::new();
    append(&mut partial, frames);
    let tail = interrupted.finish(StreamEnd::Interrupted).unwrap();
    assert!(tail.frames.is_empty());
    assert!(
        !String::from_utf8(partial)
            .unwrap()
            .contains("response.completed")
    );

    let mut failed = KiroChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: prepared.request.body(),
            response_headers: &http::HeaderMap::new(),
        })
        .unwrap();
    let frames = failed
        .push(event(
            "invalidStateEvent",
            br#"{"message":"conversation expired"}"#,
        ))
        .unwrap();
    let mut error = Vec::new();
    append(&mut error, frames);
    let error = String::from_utf8(error)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value.get("type").and_then(Value::as_str) == Some("error"))
        .expect("error event");
    assert_eq!(error["code"], "kiro_eventstream_error");
    assert!(error.get("error").is_none());
    assert!(failed.finish(StreamEnd::Complete).is_err());
}
