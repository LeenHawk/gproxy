use bytes::Bytes;
use gproxy_channel_api::{
    Channel, Disposition, PrepareCtx, ResponseShapeCtx, ResponseView, StreamDecoder, StreamEnd,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::AwsBedrockChannel;

const MESSAGES: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);
const STREAM: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

#[test]
fn declares_only_converse_and_documented_control_media_routes() {
    let supports = AwsBedrockChannel.descriptor().supports;
    assert_eq!(supports.len(), 17);
    assert_eq!(
        supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        7
    );
    assert!(supports.iter().any(|support| {
        support.source
            == OperationKey::content(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            )
            && support.target == MESSAGES
    }));
    assert!(!supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::ListModels, WireFamily::Gemini)
    }));
    assert_eq!(
        AwsBedrockChannel.classify(ResponseView {
            status: StatusCode::FORBIDDEN,
            headers: &HeaderMap::new(),
            body: &[],
        }),
        Disposition::Terminal
    );
}

#[test]
fn resolves_runtime_override_and_sigv4_control_endpoints() {
    let body = Bytes::from_static(br#"{"model":"route","max_tokens":8,"messages":[]}"#);
    let secret = json!({"api_key":"bedrock-key"});
    let settings = json!({"region":"us-west-2"});
    let runtime = AwsBedrockChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: STREAM,
            stream: true,
            method: &Method::GET,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "us.anthropic.claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        runtime.request.uri(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/model/us.anthropic.claude-sonnet-4-6/converse-stream"
    );
    assert_eq!(runtime.request.method(), Method::POST);
    assert_eq!(
        runtime.request.headers()["authorization"],
        "Bearer bedrock-key"
    );

    let override_settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{"openai_list_models":"https://models.example/all?fixed=1"}
    });
    let empty = Bytes::new();
    let iam = json!({
        "access_key_id":"AKIDEXAMPLE",
        "secret_access_key":"secret",
        "session_token":"session"
    });
    let models = AwsBedrockChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: Some("byProvider=Anthropic&ignored=yes"),
            headers: &HeaderMap::new(),
            body: &empty,
            upstream_model: "",
            provider_settings: &override_settings,
            secret: &iam,
        })
        .unwrap();
    assert_eq!(
        models.request.uri(),
        "https://models.example/all?fixed=1&byProvider=Anthropic"
    );
    assert!(
        models.request.headers()["authorization"]
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/")
    );
    assert!(models.request.headers().contains_key("x-amz-date"));
    assert!(
        models
            .request
            .headers()
            .contains_key("x-amz-security-token")
    );
    assert!(!models.request.headers().contains_key("host"));
}

#[test]
fn shapes_converse_and_strictly_decodes_fragmented_eventstream() {
    let secret = json!({"api_key":"key"});
    let settings = json!({"region":"us-east-1"});
    let body = Bytes::from_static(
        br#"{"model":"route","max_tokens":32,"speed":"fast","messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral","ttl":"1h"}}]}],"tools":[{"name":"weather","description":"Weather","input_schema":{"type":"object"}}],"tool_choice":{"type":"tool","name":"weather"}}"#,
    );
    let prepared = AwsBedrockChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "anthropic.claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["inferenceConfig"]["maxTokens"], 32);
    assert_eq!(shaped["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(
        shaped["messages"][0]["content"][1]["cachePoint"]["ttl"],
        "1h"
    );
    assert_eq!(
        shaped["toolConfig"]["tools"][0]["toolSpec"]["name"],
        "weather"
    );
    assert_eq!(shaped["serviceTier"]["type"], "priority");

    let raw = Bytes::from_static(
        br#"{"output":{"message":{"role":"assistant","content":[{"text":"ok"}]}},"stopReason":"end_turn","usage":{"inputTokens":10,"outputTokens":4,"totalTokens":14,"cacheReadInputTokens":2},"metrics":{"latencyMs":1},"serviceTier":{"type":"priority"}}"#,
    );
    let buffered = AwsBedrockChannel
        .shape_response(ResponseShapeCtx {
            key: MESSAGES,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let buffered: Value = serde_json::from_slice(&buffered).unwrap();
    assert_eq!(buffered["content"][0]["text"], "ok");
    assert_eq!(buffered["usage"]["cache_read_input_tokens"], 2);
    assert_eq!(buffered["usage"]["service_tier"], "priority");

    let events = [
        ("messageStart", json!({"role":"assistant"})),
        (
            "contentBlockStart",
            json!({"contentBlockIndex":0,"start":{}}),
        ),
        (
            "contentBlockDelta",
            json!({"contentBlockIndex":0,"delta":{"text":"ok"}}),
        ),
        ("contentBlockStop", json!({"contentBlockIndex":0})),
        ("messageStop", json!({"stopReason":"end_turn"})),
        (
            "metadata",
            json!({
                "usage":{"inputTokens":10,"outputTokens":4,"totalTokens":14,
                    "cacheReadInputTokens":2,"cacheWriteInputTokens":3,
                    "cacheDetails":[{"inputTokens":1,"ttl":"5m"},{"inputTokens":2,"ttl":"1h"}]},
                "metrics":{"latencyMs":1},"serviceTier":{"type":"priority"}
            }),
        ),
    ];
    let bytes = events
        .into_iter()
        .flat_map(|(kind, value)| smithy(kind, value))
        .collect::<Vec<_>>();
    let mut decoder = super::sse::BedrockStreamDecoder::new();
    let mut output = Vec::new();
    for chunk in bytes.chunks(11) {
        output.extend(decoder.push(Bytes::copy_from_slice(chunk)).unwrap());
    }
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    let text = output
        .iter()
        .map(|frame| String::from_utf8_lossy(&frame.0))
        .collect::<String>();
    assert!(text.contains("text_delta"));
    assert!(text.contains("message_stop"));
    let usage = tail.usage.unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cached_input_tokens
        ),
        (12, 4, 2)
    );

    let mut corrupt = smithy("messageStart", json!({"role":"assistant"}));
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(
        super::sse::BedrockStreamDecoder::new()
            .push(Bytes::from(corrupt))
            .is_err()
    );
    let mut truncated = smithy("messageStart", json!({"role":"assistant"}));
    truncated.truncate(truncated.len() - 3);
    let mut decoder = super::sse::BedrockStreamDecoder::new();
    assert!(decoder.push(Bytes::from(truncated)).unwrap().is_empty());
    assert!(decoder.finish(StreamEnd::Complete).is_err());
}

#[test]
fn invoke_messages_stream_decodes_chunk_envelopes_and_reports_free_refusals() {
    use base64::Engine;
    use gproxy_channel_api::StreamCtx;
    for output in [0, 2] {
        let events = [
            json!({"type":"message_start","message":{"id":"msg_invoke","type":"message","role":"assistant","model":"claude-fable-5","content":[],"stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
            json!({"type":"message_delta","delta":{"stop_reason":"refusal","stop_details":{"type":"refusal","category":null,"explanation":null}},"usage":{"output_tokens":output}}),
            json!({"type":"message_stop"}),
        ];
        let wire = events.iter().flat_map(|event| smithy("chunk", json!({"bytes":base64::engine::general_purpose::STANDARD.encode(event.to_string())}))).collect::<Vec<_>>();
        let request = Bytes::from_static(br#"{"anthropic_version":"bedrock-2023-05-31"}"#);
        let mut decoder = AwsBedrockChannel
            .stream_decoder(StreamCtx {
                key: STREAM,
                framing: gproxy_protocol::StreamFraming::Sse,
                request_body: &request,
                response_headers: &HeaderMap::new(),
            })
            .unwrap();
        let mut frames = Vec::new();
        for chunk in wire.chunks(13) {
            frames.extend(decoder.push(Bytes::copy_from_slice(chunk)).unwrap());
        }
        let tail = decoder.finish(StreamEnd::Complete).unwrap();
        let usage = tail.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, output);
        assert_eq!(usage.attempts[0].billable, output > 0);
        let text = frames
            .iter()
            .map(|frame| String::from_utf8_lossy(&frame.0))
            .collect::<String>();
        assert!(text.contains("message_stop"));
        assert!(text.contains("stop_details"));
    }
}

fn smithy(event: &str, payload: Value) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [
        (":message-type", "event"),
        (":event-type", event),
        (":content-type", "application/json"),
    ] {
        headers.push(name.len() as u8);
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let payload = serde_json::to_vec(&payload).unwrap();
    let total = 12 + headers.len() + payload.len() + 4;
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(total as u32).to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32fast::hash(&frame).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&crc32fast::hash(&frame).to_be_bytes());
    frame
}
