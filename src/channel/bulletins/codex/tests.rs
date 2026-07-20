mod auth;

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::{CodexChannel, model_metadata, usage};
use crate::channel::usage::RateLimitResetCreditConsumeOutcome;
use crate::channel::{Channel, ChannelLogin, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKind, Provider};
use crate::transform::routing::RoutingDecision;

struct NoopUpstream;

#[async_trait::async_trait]
impl UpstreamClient for NoopUpstream {
    async fn send(
        &self,
        _request: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, crate::http::client::ClientError> {
        Err(crate::http::client::ClientError::Transport("noop".into()))
    }
}

fn shaped_body(body: &'static [u8]) -> Value {
    let settings = json!({});
    let context = ShapeCtx {
        op: crate::protocol::OperationKey::content_generation(
            Operation::StreamGenerateContent,
            Kind::OpenAiResponses,
        ),
        stream: true,
        status: StatusCode::OK,
        settings: &settings,
    };
    let body =
        CodexChannel.shape_request(Bytes::from_static(body), &mut HeaderMap::new(), &context);
    serde_json::from_slice(&body).unwrap()
}

fn route(operation: Operation, kind: Kind) -> RoutingDecision {
    CodexChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| {
            source.operation == operation && source.kind == crate::channel::routes::cg(kind)
        })
        .map(|(_, decision)| decision)
        .expect("missing route")
}

fn provider_route(operation: Operation, provider: Provider) -> RoutingDecision {
    CodexChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| {
            source.operation == operation && source.kind == crate::channel::routes::pv(provider)
        })
        .map(|(_, decision)| decision)
        .expect("missing route")
}

#[test]
fn stream_decoder_backfills_function_call_in_completed_output() {
    let item = json!({
        "type": "response.output_item.done",
        "output_index": 0,
        "item": {
            "id": "fc_1",
            "type": "function_call",
            "status": "completed",
            "call_id": "call_1",
            "name": "get_me_mcp_github",
            "arguments": "{}"
        }
    });
    let completed = json!({
        "type": "response.completed",
        "response": { "id": "resp_1", "object": "response", "status": "completed", "output": [] }
    });
    let upstream = format!(
        "event: response.output_item.done\ndata: {item}\n\n\
         event: response.completed\ndata: {completed}\n\n"
    );

    let mut decoder = CodexChannel.stream_decoder().expect("codex normalizer");
    let mut normalized = decoder.push(upstream.as_bytes());
    normalized.extend(decoder.finish());
    let mut sse = crate::transform::common::sse::SseDecoder::new();
    let completed: Value = sse
        .push(&normalized)
        .into_iter()
        .find(|frame| frame.event.as_deref() == Some("response.completed"))
        .map(|frame| serde_json::from_str(&frame.data).unwrap())
        .expect("completed event");
    assert_eq!(completed["response"]["output"][0], item["item"]);
}

#[test]
fn magic_cache_breakpoint_survives_codex_normalization() {
    let mut request_headers = HeaderMap::new();
    let shape_settings = json!({ "enable_magic_cache": true });
    let context = ShapeCtx {
        op: crate::protocol::OperationKey::content_generation(
            Operation::StreamGenerateContent,
            Kind::OpenAiResponses,
        ),
        stream: true,
        status: StatusCode::OK,
        settings: &shape_settings,
    };
    let shaped = CodexChannel.shape_request(
        Bytes::from_static(
            br#"{"model":"gpt-5.6","instructions":"stable GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH","input":"hello"}"#,
        ),
        &mut request_headers,
        &context,
    );
    let secret = json!({ "access_token": "tok-abc" });
    let provider_settings = json!({});
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &provider_settings,
            upstream_model_id: "gpt-5.6",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &request_headers,
            body: shaped,
        })
        .unwrap()
        .into_http();
    let value: Value = serde_json::from_slice(prepared.body()).unwrap();
    assert_eq!(
        value["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(value["input"][0]["role"], "developer");
}

#[test]
fn content_defaults_target_streaming_responses_except_websocket_source() {
    for (operation, kind) in [
        (Operation::GenerateContent, Kind::OpenAiResponses),
        (Operation::GenerateContent, Kind::OpenAiChatCompletions),
        (Operation::GenerateContent, Kind::ClaudeMessages),
        (Operation::GenerateContent, Kind::GeminiGenerateContent),
        (
            Operation::StreamGenerateContent,
            Kind::OpenAiChatCompletions,
        ),
        (Operation::StreamGenerateContent, Kind::ClaudeMessages),
        (
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    ] {
        let RoutingDecision::TransformTo(target) = route(operation, kind) else {
            panic!("route should transform to streaming responses");
        };
        assert_eq!(target.operation, Operation::StreamGenerateContent);
        assert_eq!(
            target.kind,
            OperationKind::ContentGeneration(Kind::OpenAiResponses)
        );
    }
    assert_eq!(
        route(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        RoutingDecision::Passthrough
    );
    assert_eq!(
        route(
            Operation::StreamGenerateContent,
            Kind::OpenAiResponsesWebSocket
        ),
        RoutingDecision::Passthrough
    );
    let RoutingDecision::TransformTo(target) =
        route(Operation::GenerateContent, Kind::OpenAiResponsesWebSocket)
    else {
        panic!("websocket source should transform to streaming websocket");
    };
    assert_eq!(target.operation, Operation::StreamGenerateContent);
    assert_eq!(
        target.kind,
        OperationKind::ContentGeneration(Kind::OpenAiResponsesWebSocket)
    );
}

#[test]
fn embeddings_default_to_unsupported() {
    assert_eq!(
        provider_route(Operation::CreateEmbedding, Provider::OpenAi),
        RoutingDecision::Unsupported
    );
    assert_eq!(
        provider_route(Operation::CreateEmbedding, Provider::Gemini),
        RoutingDecision::Unsupported
    );
}

#[test]
fn prepare_responses_websocket_returns_custom_stream() {
    let secret = json!({ "access_token": "tok-abc" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let context = PrepareCtx {
        secret: &secret,
        provider_settings: &settings,
        upstream_model_id: "gpt-5.4",
        method: Method::GET,
        path: "/v1/responses",
        query: None,
        headers: &headers,
        body: Bytes::from_static(
            br#"{"type":"response.create","model":"gpt-5.4","input":"hi","stream":true}"#,
        ),
    };
    assert!(matches!(
        CodexChannel.prepare(context).unwrap(),
        PreparedRequest::CustomStream(_)
    ));
}

#[test]
fn normalizes_responses_body() {
    let value = shaped_body(
        br#"{"model":"gpt-5.4","input":"hi","temperature":0.7,"max_output_tokens":100,"stream":false}"#,
    );
    assert_eq!(value["stream"], true);
    assert_eq!(value["store"], false);
    assert!(value.get("temperature").is_none());
    assert!(value.get("max_output_tokens").is_none());
    assert_eq!(
        value["input"],
        json!([{ "type": "message", "role": "user", "content": "hi" }])
    );

    let value = shaped_body(
        br#"{"model":"gpt-5.4","input":[{"role":"system","content":"S"},{"role":"user","content":"U"}]}"#,
    );
    assert_eq!(value["instructions"], "S");
    let roles: Vec<&str> = value["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| message["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user"]);
}

#[test]
fn prepare_url_body_and_headers() {
    let secret = json!({ "access_token": "tok-abc", "account_id": "acct-9" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let body = Bytes::from_static(br#"{"model":"gpt-5.4","input":"hi"}"#);
    let request = CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: body.clone(),
        })
        .unwrap()
        .into_http();
    assert_eq!(request.body(), &body, "prepare must preserve body bytes");
    assert_eq!(
        request.uri().to_string(),
        "https://chatgpt.com/backend-api/codex/responses"
    );
    assert_eq!(request.headers()["authorization"], "Bearer tok-abc");
    assert_eq!(request.headers()["originator"], "codex_exec");
    assert_eq!(request.headers()["chatgpt-account-id"], "acct-9");
    assert_eq!(
        request.headers()["session-id"],
        request.headers()["x-client-request-id"]
    );
}

#[test]
fn model_list_request_carries_client_version() {
    let secret = json!({ "access_token": "tok-abc" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let request = CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http();
    assert_eq!(
        request.uri().to_string(),
        format!(
            "https://chatgpt.com/backend-api/codex/models?client_version={}",
            model_metadata::CODEX_VERSION
        )
    );
    assert!(request.body().is_empty());
}

#[test]
fn forwards_codex_client_headers() {
    let secret = json!({ "access_token": "tok-abc" });
    let settings = json!({});
    let id = "019ebb45-a25d-7520-a8e3-fda4ebc99692";
    let mut headers = HeaderMap::new();
    headers.insert("session-id", id.parse().unwrap());
    headers.insert("thread-id", id.parse().unwrap());
    headers.insert("x-client-request-id", id.parse().unwrap());
    headers.insert("x-codex-window-id", format!("{id}:0").parse().unwrap());
    headers.insert(
        "x-codex-beta-features",
        "terminal_resize_reflow,memories".parse().unwrap(),
    );
    let request = CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"input":"hi"}"#),
        })
        .unwrap()
        .into_http();
    assert_eq!(request.headers()["session-id"], id);
    assert_eq!(request.headers()["thread-id"], id);
    assert_eq!(request.headers()["x-client-request-id"], id);
    assert_eq!(request.headers()["x-codex-window-id"], format!("{id}:0"));
    assert_eq!(
        request.headers()["x-codex-beta-features"],
        "terminal_resize_reflow,memories"
    );
    assert_eq!(request.headers()["originator"], "codex_exec");
}

#[tokio::test]
async fn codex_authcode_start_url() {
    let client: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);
    let start = CodexChannel
        .authcode_start(&client, &json!({}), "", "STATE", "CHAL")
        .await
        .expect("authcode_start ok")
        .expect("codex supports authcode");
    let url = &start.authorize_url;
    assert!(url.starts_with("https://auth.openai.com/oauth/authorize?"));
    assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
    assert!(url.contains("code_challenge=CHAL"));
    assert!(url.contains("state=STATE"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("redirect_uri="));
    assert_eq!(start.redirect_uri, "http://localhost:1455/auth/callback");
}

#[test]
fn shapes_codex_model_metadata() {
    let list = Bytes::from_static(
        br#"{"models":[{"slug":"gpt-5.4-codex"},{"id":"gpt-5.4"},{"name":"no-id"}]}"#,
    );
    let value: Value = serde_json::from_slice(&model_metadata::shape_model_list(list)).unwrap();
    assert_eq!(value["object"], "list");
    assert_eq!(value["data"].as_array().unwrap().len(), 2);
    assert_eq!(value["data"][0]["id"], "gpt-5.4-codex");
    assert_eq!(value["data"][1]["id"], "gpt-5.4");

    let get = Bytes::from_static(br#"{"models":[{"slug":"gpt-5.4-codex"}]}"#);
    let value: Value = serde_json::from_slice(&model_metadata::shape_model_get(get)).unwrap();
    assert_eq!(value["id"], "gpt-5.4-codex");
    assert_eq!(value["object"], "model");
}

#[test]
fn model_metadata_passthrough_on_non_codex_shape() {
    let canonical = Bytes::from_static(br#"{"object":"list","data":[{"id":"gpt-5.4"}]}"#);
    assert_eq!(
        model_metadata::shape_model_list(canonical.clone()),
        canonical
    );
    let invalid = Bytes::from_static(b"not json");
    assert_eq!(model_metadata::shape_model_get(invalid.clone()), invalid);
}

#[test]
fn parses_rate_limit_payload() {
    let body = Bytes::from_static(
        br#"{
          "plan_type": "pro",
          "rate_limit": {
            "primary_window": {"used_percent": 42, "limit_window_seconds": 300, "reset_at": 1704069000},
            "secondary_window": {"used_percent": 84, "limit_window_seconds": 604800, "reset_at": 1704074400}
          },
          "additional_rate_limits": [{
            "limit_name": "GPT-5.3-Codex-Spark",
            "metered_feature": "codex_bengalfox",
            "rate_limit": {
              "primary_window": {"used_percent": 0, "limit_window_seconds": 18000, "reset_at": 1783156510},
              "secondary_window": {"used_percent": 9, "limit_window_seconds": 604800, "reset_at": 1783650621}
            }
          }],
          "credits": {"has_credits": true, "unlimited": false, "balance": "9.99"},
          "rate_limit_reset_credits": {"available_count": 2}
        }"#,
    );
    let snapshot = usage::parse(StatusCode::OK, &body).expect("snapshot");
    assert_eq!(snapshot.plan.as_deref(), Some("pro"));
    assert_eq!(snapshot.windows.len(), 4);
    assert_eq!(snapshot.windows[0].name, "primary");
    assert_eq!(snapshot.windows[0].used_percent, Some(42.0));
    assert_eq!(snapshot.windows[0].window_seconds, Some(300));
    assert_eq!(snapshot.windows[0].resets_at_unix, Some(1704069000));
    assert_eq!(
        snapshot.windows[2].name,
        "additional_primary:codex_bengalfox"
    );
    assert_eq!(
        snapshot.windows[2].label.as_deref(),
        Some("GPT-5.3-Codex-Spark")
    );
    assert_eq!(snapshot.windows[3].used_percent, Some(9.0));
    assert_eq!(snapshot.credits.unwrap().balance.as_deref(), Some("9.99"));
    assert_eq!(
        snapshot
            .rate_limit_reset_credits
            .expect("reset credits")
            .available_count,
        2
    );
}

#[test]
fn parses_rate_limit_reset_credit_response() {
    let body = Bytes::from_static(br#"{"code":"reset","windows_reset":1}"#);
    let response = usage::parse_reset_credit(StatusCode::OK, &body).expect("reset response");
    assert_eq!(response.outcome, RateLimitResetCreditConsumeOutcome::Reset);
    assert_eq!(response.windows_reset, Some(1));
}
