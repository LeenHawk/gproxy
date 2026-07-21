use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, Response, StatusCode};
use serde_json::{Value, json};

use super::{TaskletChannel, bridge, mcp, request, response, stream};
use crate::channel::{Channel, PrepareCtx, PreparedRequest};
use crate::http::client::{ClientError, ConduitSocket, RespStream, UpstreamClient};
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, Provider};
use crate::transform::routing::RoutingDecision;

#[test]
fn non_stream_chat_routes_to_streaming_chat() {
    let decision = TaskletChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| {
            source.operation == Operation::GenerateContent
                && source.kind
                    == OperationKind::ContentGeneration(
                        ContentGenerationKind::OpenAiChatCompletions,
                    )
        })
        .map(|(_, decision)| decision)
        .unwrap();
    let RoutingDecision::TransformTo(target) = decision else {
        panic!("tasklet non-stream route must aggregate its stream")
    };
    assert_eq!(target.operation, Operation::StreamGenerateContent);
}

#[test]
fn parses_inline_attachment_for_upload() {
    let body = json!({
        "model":"tasklet-standard",
        "messages":[{"role":"user","content":[
            {"type":"text","text":"describe"},
            {"type":"image_url","image_url":{"url":"data:image/png;base64,aW1n"}}
        ]}]
    });
    let parsed = request::parse(body.to_string().as_bytes(), "tasklet-standard").unwrap();
    assert_eq!(parsed.message, "describe");
    assert_eq!(parsed.uploads.len(), 1);
    assert_eq!(parsed.uploads[0].bytes, b"img");
}

#[test]
fn captures_client_tools_and_attaches_bridge_instructions() {
    let body = json!({
        "model":"tasklet-standard",
        "messages":[{"role":"user","content":"check the weather"}],
        "tools":[{"type":"function","function":{
            "name":"get_weather",
            "description":"Get weather",
            "parameters":{"type":"object","properties":{"city":{"type":"string"}}}
        }}],
        "tool_choice":"required"
    });
    let mut parsed = request::parse(body.to_string().as_bytes(), "tasklet-standard").unwrap();
    assert_eq!(parsed.tools.len(), 1);
    request::attach_tool_bridge(&mut parsed, "turn_test").unwrap();
    assert!(parsed.message.contains("gproxy_call_client_tool"));
    assert!(parsed.message.contains("turn_test"));
    assert!(parsed.message.contains("get_weather"));
}

struct MockSocket {
    received: VecDeque<Result<String, ClientError>>,
    sent: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl ConduitSocket for MockSocket {
    async fn send_text(&mut self, text: String) -> Result<(), ClientError> {
        self.sent
            .lock()
            .unwrap()
            .push(serde_json::from_str(&text).unwrap());
        Ok(())
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        self.received.pop_front()
    }
}

struct PendingSocket;

#[async_trait]
impl ConduitSocket for PendingSocket {
    async fn send_text(&mut self, _text: String) -> Result<(), ClientError> {
        Ok(())
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        std::future::pending().await
    }
}

struct MockClient {
    requests: Mutex<Vec<Request<Bytes>>>,
    sent_frames: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl UpstreamClient for MockClient {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        self.requests.lock().unwrap().push(request);
        Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from_static(br#"{"agentId":"a_mock"}"#))
            .map_err(|error| ClientError::Transport(error.to_string()))
    }

    async fn open_websocket(
        &self,
        _request: Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        let frames = [
            json!({"type":"connected"}),
            json!({"type":"syncUpdate","state":{"runState":{"type":"ready"}}}),
            json!({"type":"syncUpdate","state":{"runState":{"type":"running"}}}),
            json!({"type":"blocksUpdate","updates":{"b_1":{
                "type":"thinking","blockId":"b_1","content":"Plan"
            }}}),
            json!({"type":"blocksUpdate","updates":{"b_1":{
                "type":"thinking","blockId":"b_1","content":"Plan done"
            },"b_2":{
                "type":"agent_content","blockId":"b_2","content":"Hello"
            }}}),
            json!({"type":"syncUpdate","state":{"runState":{"type":"idle"}}}),
        ]
        .into_iter()
        .map(|frame| Ok(frame.to_string()))
        .collect();
        Ok(Box::new(MockSocket {
            received: frames,
            sent: Arc::clone(&self.sent_frames),
        }))
    }
}

#[tokio::test]
async fn runs_agent_websocket_and_emits_openai_sse() {
    let client = Arc::new(MockClient {
        requests: Mutex::new(Vec::new()),
        sent_frames: Arc::new(Mutex::new(Vec::new())),
    });
    let client_dyn: Arc<dyn UpstreamClient> = client.clone();
    let secret = json!({"session_token":"token","workspace_id":"ws_mock"});
    let settings = json!({});
    let headers = http::HeaderMap::new();
    let body = Bytes::from(
        json!({
            "model":"tasklet-standard",
            "stream":true,
            "messages":[{"role":"user","content":"hello"}]
        })
        .to_string(),
    );
    let prepared = TaskletChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey {
                operation: Operation::StreamGenerateContent,
                kind: OperationKind::ContentGeneration(
                    ContentGenerationKind::OpenAiChatCompletions,
                ),
            },
            stream: true,
            upstream_model_id: "tasklet-standard",
            method: http::Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body,
        })
        .unwrap();
    let PreparedRequest::CustomStream(send) = prepared else {
        panic!("tasklet must use CustomStream")
    };
    let (status, _, mut stream): (_, _, RespStream) = send(client_dyn).await.unwrap();
    assert_eq!(status, StatusCode::OK);
    let mut output = Vec::new();
    while let Some(chunk) = stream.next().await {
        output.extend_from_slice(&chunk.unwrap());
    }
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("\"role\":\"assistant\""));
    assert!(output.contains("\"reasoning_content\":\"Plan\""));
    assert!(output.contains("\"reasoning_content\":\" done\""));
    assert!(output.contains("\"content\":\"Hello\""));
    assert!(output.ends_with("data: [DONE]\n\n"));

    let sent = client.sent_frames.lock().unwrap();
    assert_eq!(sent[0]["type"], "connect");
    assert_eq!(sent[1]["type"], "startSync");
    assert_eq!(sent[2]["type"], "subscribeBlocks");
    assert_eq!(sent[1]["agentId"], sent[2]["runId"]);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests[0].uri().path(), "/api/sendChatMessage");
    assert_eq!(requests[0].headers()["authorization"], "Bearer token");
    let sent_body: Value = serde_json::from_slice(requests[0].body()).unwrap();
    assert_eq!(sent_body["workspaceId"], "ws_mock");
    assert_eq!(sent_body["intelligence"], "standard");
}

#[tokio::test]
async fn mcp_bridge_emits_openai_tool_call() {
    let tools: Vec<crate::protocol::openai::ChatTool> = serde_json::from_value(json!([{
        "type":"function",
        "function":{"name":"get_weather","parameters":{"type":"object"}}
    }]))
    .unwrap();
    let turn = bridge::register(&tools).unwrap();
    let turn_id = turn.id().to_owned();
    let mut output = stream::create(
        Box::new(PendingSocket),
        response::Synth::new("tasklet-standard".into(), false),
        Some(turn),
    )
    .unwrap();
    let delegated = tokio::spawn(async move {
        bridge::dispatch(&turn_id, "get_weather".into(), json!({"city":"Paris"})).await
    });
    let mut bytes = Vec::new();
    while let Some(chunk) = output.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    delegated.await.unwrap().unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"name\":\"get_weather\""));
    assert!(text.contains("\\\"city\\\":\\\"Paris\\\""));
    assert!(text.contains("\"finish_reason\":\"tool_calls\""));
}

#[tokio::test]
async fn bundled_mcp_service_lists_bridge_tool() {
    use axum::Router;
    use axum::body::Body;
    use http_body_util::BodyExt as _;
    use tower::ServiceExt as _;

    let app = Router::new().nest_service("/tasklet/mcp", mcp::service());
    let response = app
        .oneshot(
            Request::post("/tasklet/mcp")
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("host", "localhost")
                .header("mcp-protocol-version", "2025-03-26")
                .body(Body::from(
                    r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value["result"]["tools"][0]["name"],
        "gproxy_call_client_tool"
    );
}

#[test]
fn bundled_catalog_is_openai_shaped() {
    let body = TaskletChannel.bundled_models().unwrap();
    let catalog: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(catalog["object"], "list");
    assert!(
        catalog["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|model| model["id"] == "tasklet-standard")
    );
    assert_eq!(Provider::OpenAi, TaskletChannel.provider_family());
}
