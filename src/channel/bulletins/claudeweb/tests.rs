use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, Response, StatusCode};
use serde_json::{Value, json};

use super::{ClaudeWebChannel, auth, models};
use crate::channel::{Channel, ChannelLogin, PrepareCtx, PreparedRequest};
use crate::http::client::{ClientError, RespStream, UpstreamClient};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind};
use crate::transform::routing::RoutingDecision;

#[test]
fn non_stream_messages_force_web_stream() {
    let decision = ClaudeWebChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| {
            source.operation == Operation::GenerateContent
                && source.kind
                    == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        })
        .map(|(_, decision)| decision)
        .unwrap();
    let RoutingDecision::TransformTo(target) = decision else {
        panic!("claudeweb non-stream route should aggregate a web stream")
    };
    assert_eq!(target.operation, Operation::StreamGenerateContent);
    assert_eq!(
        target.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    );
}

#[test]
fn normalizes_cookie_header_and_bare_value() {
    assert_eq!(
        auth::normalize_session_key("foo=bar; sessionKey=sk-ant-sid01-example; x=y").as_deref(),
        Some("sk-ant-sid01-example")
    );
    assert_eq!(
        auth::normalize_session_key("sk-ant-sid02-example").as_deref(),
        Some("sk-ant-sid02-example")
    );
}

#[test]
fn preserves_full_browser_cookie_and_device_id() {
    let cookie = auth::normalize_cookie(
        "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example; anthropic-device-id=device-1",
    )
    .unwrap();
    assert_eq!(
        cookie,
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example; anthropic-device-id=device-1"
    );
    assert_eq!(
        auth::cookie_value(&cookie, "anthropic-device-id"),
        Some("device-1")
    );
    assert_eq!(
        auth::normalize_cookie("sk-ant-sid02-example").as_deref(),
        Some("sk-ant-sid02-example")
    );
}

#[test]
fn refreshes_missing_or_stale_validation_only() {
    let now = auth::now_ms();
    assert!(auth::needs_refresh(
        &json!({"cookie": "sk-ant-sid-example"})
    ));
    assert!(!auth::needs_refresh(&json!({
        "validated_at_ms": now - auth::VALIDATION_INTERVAL_MS + 1_000
    })));
    assert!(auth::needs_refresh(&json!({
        "validated_at_ms": now - auth::VALIDATION_INTERVAL_MS - 1_000
    })));
}

#[test]
fn extracts_models_from_array_and_keyed_configs() {
    let catalog = models::catalog(&json!({
        "models": [
            {"model": "claude-sonnet-5", "display_name": "Claude Sonnet 5"},
            {"id": "claude-fable-5", "label": "Claude Fable 5"}
        ],
        "claude-opus-4-8": {"name": "Claude Opus 4.8"},
        "unrelated": {"id": "not-a-claude-model"}
    }))
    .unwrap();
    let data = catalog["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    assert!(data.iter().any(|model| {
        model["id"] == "claude-sonnet-5" && model["display_name"] == "Claude Sonnet 5"
    }));
    assert!(data.iter().any(|model| {
        model["id"] == "claude-opus-4-8" && model["display_name"] == "Claude Opus 4.8"
    }));
}

struct ToolFlowClient {
    requests: Mutex<Vec<(String, Bytes)>>,
}

#[async_trait]
impl UpstreamClient for ToolFlowClient {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        let path = request.uri().path().to_owned();
        let body = request.body().clone();
        self.requests.lock().unwrap().push((path.clone(), body));
        let status =
            if request.method() == http::Method::POST && path.ends_with("/chat_conversations") {
                StatusCode::CREATED
            } else if request.method() == http::Method::DELETE {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::OK
            };
        Response::builder()
            .status(status)
            .body(Bytes::new())
            .map_err(|error| ClientError::Transport(error.to_string()))
    }

    async fn send_streaming(
        &self,
        request: Request<Bytes>,
    ) -> Result<(StatusCode, http::HeaderMap, RespStream), ClientError> {
        self.requests
            .lock()
            .unwrap()
            .push((request.uri().path().to_owned(), request.body().clone()));
        let first = Bytes::from_static(
            b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_mock\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-5\"}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_mock\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"Singapore\\\"}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        );
        let second = Bytes::from_static(
            b"data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu_mock\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Sunny.\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let stream = futures_util::stream::iter([Ok(first), Ok(second)]).boxed();
        Ok((StatusCode::OK, http::HeaderMap::new(), stream))
    }
}

async fn collect(mut stream: RespStream) -> String {
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn tool_result_resumes_parked_stream_and_synthesizes_usage() {
    let client = Arc::new(ToolFlowClient {
        requests: Mutex::new(Vec::new()),
    });
    let client_dyn: Arc<dyn UpstreamClient> = client.clone();
    let channel = ClaudeWebChannel;
    let settings = json!({});
    let secret = json!({
        "cookie":"sk-ant-sid-example",
        "account_uuid":"org-1",
        "capabilities":["chat"],
        "device_id":"00000000-0000-4000-8000-000000000001"
    });
    let headers = http::HeaderMap::new();
    let first_body = Bytes::from(
        json!({
            "model":"claude-sonnet-5",
            "max_tokens":128,
            "messages":[{"role":"user","content":"Use the weather tool"}],
            "tools":[{"name":"weather","description":"weather","input_schema":{"type":"object"}}]
        })
        .to_string(),
    );
    let first = channel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-5",
            method: http::Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: first_body,
        })
        .unwrap();
    let PreparedRequest::CustomStream(send) = first else {
        panic!("claudeweb should use a custom stream")
    };
    let (_, _, first_stream) = send(Arc::clone(&client_dyn)).await.unwrap();
    let first_text = collect(first_stream).await;
    assert!(first_text.contains("\"id\":\"toolu_mock\""));
    assert!(first_text.contains("\"stop_reason\":\"tool_use\""));
    assert!(first_text.contains("\"input_tokens\":"));

    let second_body = Bytes::from(
        json!({
            "model":"claude-sonnet-5",
            "max_tokens":128,
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":"toolu_mock","name":"weather","input":{"city":"Singapore"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_mock","content":"Sunny, 31 C"}]}
            ]
        })
        .to_string(),
    );
    let second = channel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-5",
            method: http::Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: second_body,
        })
        .unwrap();
    let PreparedRequest::CustomStream(send) = second else {
        panic!("tool result should resume a custom stream")
    };
    let (_, _, second_stream) = send(client_dyn).await.unwrap();
    let second_text = collect(second_stream).await;
    assert!(second_text.contains("event: message_start"));
    assert!(second_text.contains("Sunny."));
    assert!(!second_text.contains("\"type\":\"tool_result\""));
    assert!(second_text.contains("\"output_tokens\":"));

    let requests = client.requests.lock().unwrap();
    let result_request = requests
        .iter()
        .find(|(path, _)| path.ends_with("/tool_result"))
        .expect("tool_result request");
    let sent: Value = serde_json::from_slice(&result_request.1).unwrap();
    assert_eq!(sent["tool_use_id"], "toolu_mock");
    assert_eq!(sent["content"][0]["text"], "Sunny, 31 C");
}

#[tokio::test]
#[ignore = "requires CLAUDE_SESSION_KEY and live claude.ai access"]
async fn live_tool_result_round_trip() {
    let cookie = std::env::var("CLAUDE_SESSION_KEY").expect("CLAUDE_SESSION_KEY");
    let organization = std::env::var("CLAUDE_ORGANIZATION_UUID").ok();
    let device_id = std::env::var("CLAUDE_DEVICE_ID").ok();
    let channel = ClaudeWebChannel;
    let client: Arc<dyn UpstreamClient> = Arc::new(
        crate::http::client::WreqClient::with_proxy_and_emulation(
            None,
            channel.default_emulation(),
        )
        .expect("browser client"),
    );
    let secret = match organization {
        Some(organization) => json!({
            "cookie":cookie,
            "account_uuid":organization,
            "capabilities":["chat"],
            "device_id":device_id.unwrap_or_else(crate::util::rand::uuid_v4)
        }),
        None => channel.cookie_exchange(&client, &cookie).await.unwrap(),
    };
    let settings = json!({"timezone":"Asia/Singapore"});
    let headers = http::HeaderMap::new();
    let first_body = Bytes::from(
        json!({
            "model":"claude-sonnet-5",
            "max_tokens":256,
            "messages":[{"role":"user","content":"Call get_weather exactly once for Singapore, then wait for its result."}],
            "tools":[{"name":"get_weather","description":"Get current weather","input_schema":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}]
        })
        .to_string(),
    );
    let first = channel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-5",
            method: http::Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: first_body,
        })
        .unwrap();
    let PreparedRequest::CustomStream(send) = first else {
        panic!("custom stream")
    };
    let (status, _, stream) = send(Arc::clone(&client)).await.unwrap();
    let first_text = collect(stream).await;
    assert_eq!(status, StatusCode::OK, "{first_text}");
    let tool_use_id = first_text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|event| {
            event
                .get("content_block")
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                .and_then(|block| block.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .expect("live tool_use");
    assert!(first_text.contains("\"stop_reason\":\"tool_use\""));

    let second_body = Bytes::from(
        json!({
            "model":"claude-sonnet-5",
            "max_tokens":256,
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":tool_use_id,"name":"get_weather","input":{"city":"Singapore"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":tool_use_id,"content":"Sunny, 31 C"}]}
            ]
        })
        .to_string(),
    );
    let second = channel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-5",
            method: http::Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: second_body,
        })
        .unwrap();
    let PreparedRequest::CustomStream(send) = second else {
        panic!("resumed custom stream")
    };
    let (status, _, stream) = send(client).await.unwrap();
    let second_text = collect(stream).await;
    assert_eq!(status, StatusCode::OK, "{second_text}");
    assert!(
        second_text.contains("\"type\":\"text_delta\""),
        "{second_text}"
    );
    assert!(second_text.contains("\"stop_reason\":\"end_turn\""));
    assert!(second_text.contains("event: message_stop"));
    assert!(second_text.contains("\"output_tokens\":"));
    assert!(!second_text.contains("\"type\":\"tool_result\""));
}
