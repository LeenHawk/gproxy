use bytes::Bytes;
use gproxy_channel_api::{
    Channel, ChannelSupport, DriverInput, OperationStep, OperationStream, PrepareCtx, StepResponse,
};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::ClaudeWebChannel;
use super::stream::{Codec, SessionState};

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

fn secret() -> Value {
    json!({
        "cookie":"session-cookie",
        "account_uuid":"org-1",
        "capabilities":["chat","pro"]
    })
}

#[test]
fn declares_eight_transform_after_content_operations() {
    let expected = [
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::ClaudeMessages)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::OpenAiChat),
            content(Operation::GenerateContent, Kind::ClaudeMessages),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::OpenAiResponses),
            content(Operation::GenerateContent, Kind::ClaudeMessages),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::ClaudeMessages),
        ),
        ChannelSupport::passthrough(content(
            Operation::StreamGenerateContent,
            Kind::ClaudeMessages,
        )),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        ),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        ),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        ),
    ];
    assert_eq!(ClaudeWebChannel.descriptor().supports, expected);
    assert!(ClaudeWebChannel.requires_continuations());
}

#[test]
fn driver_uses_default_and_exact_step_urls() {
    let body =
        Bytes::from_static(br#"{"model":"claude","messages":[{"role":"user","content":"hello"}]}"#);
    let headers = HeaderMap::new();
    let secret = secret();
    let defaults = json!({});
    let mut default = ClaudeWebChannel
        .operation_driver(PrepareCtx {
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-sonnet-4-6",
            provider_settings: &defaults,
            secret: &secret,
        })
        .unwrap()
        .unwrap();
    let OperationStep::Call { request, .. } = default.next(None).unwrap() else {
        panic!("new turn must create a conversation")
    };
    assert_eq!(
        request.request.uri().path(),
        "/api/organizations/org-1/chat_conversations"
    );

    let settings = json!({
        "endpoints":{
            "claudeweb_conversation_create":"https://override.example/org/{organization}/new",
            "claudeweb_conversation_settings":"https://override.example/c/{conversation}/settings",
            "claudeweb_completion":"https://override.example/c/{conversation}/completion"
        }
    });
    let mut driver = ClaudeWebChannel
        .operation_driver(PrepareCtx {
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap()
        .unwrap();
    let OperationStep::Call { request, .. } = driver.next(None).unwrap() else {
        panic!("create step")
    };
    assert_eq!(request.request.uri().host(), Some("override.example"));
    assert!(request.request.uri().path().starts_with("/org/org-1/new"));
    let ok = || {
        DriverInput::Response(StepResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        })
    };
    let OperationStep::Call { request, .. } = driver.next(Some(ok())).unwrap() else {
        panic!("settings step")
    };
    assert!(request.request.uri().path().ends_with("/settings"));
    let OperationStep::Final { request, .. } = driver.next(Some(ok())).unwrap() else {
        panic!("completion step")
    };
    assert!(request.request.uri().path().ends_with("/completion"));
}

#[test]
fn shapes_web_turn_and_parks_then_resumes_tool_stream() {
    let request = json!({
        "system":"be concise",
        "messages":[{"role":"user","content":"use weather"}],
        "tools":[{"name":"weather","input_schema":{"type":"object"}}]
    });
    let web = super::request::build(&request, "claude-opus-4-8-thinking", "", "UTC").unwrap();
    assert_eq!(web.body["model"], "claude-opus-4-8");
    assert_eq!(web.body["thinking_mode"], "auto");
    assert!(
        web.body["attachments"][0]["extracted_content"]
            .as_str()
            .unwrap()
            .contains("use weather")
    );

    let state = SessionState {
        conversation: "conversation-1".into(),
        model: "claude-opus-4-8".into(),
        message_id: "msg-1".into(),
        input_tokens: 8,
    };
    let mut codec = Codec::new(state, false);
    let output = codec.push(Bytes::from_static(
        b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-up\",\"content\":[]}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu-1\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    )).unwrap();
    let pause = output.pause.expect("tool boundary pauses");
    assert_eq!(pause.id, "toolu-1");
    let text = output
        .frames
        .iter()
        .map(|frame| String::from_utf8_lossy(&frame.0))
        .collect::<String>();
    assert!(text.contains("message_stop"));

    let state: SessionState = serde_json::from_value(pause.state).unwrap();
    let mut resumed = Codec::new(state, true);
    let output = resumed.push(Bytes::from_static(
        b"data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu-1\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Sunny\"}}\n\n",
    )).unwrap();
    let text = output
        .frames
        .iter()
        .map(|frame| String::from_utf8_lossy(&frame.0))
        .collect::<String>();
    assert!(text.contains("message_start"));
    assert!(text.contains("Sunny"));
    assert!(!text.contains("tool_result"));
}
