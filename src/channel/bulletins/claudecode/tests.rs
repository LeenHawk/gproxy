use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method, Response};
use serde_json::{Value, json};

use super::{ClaudeCodeChannel, auth, request};
use crate::channel::{Channel, ChannelLogin, PrepareCtx, ShapeCtx};
use crate::http::client::{ClientError, UpstreamClient};
use crate::protocol::{ContentGenerationKind, Provider};

struct MockUpstream;

#[async_trait::async_trait]
impl UpstreamClient for MockUpstream {
    async fn send(&self, _request: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        Ok(Response::builder()
            .status(200)
            .body(Bytes::from_static(
                br#"{"access_token":"new","refresh_token":"newrt","expires_in":3600}"#,
            ))
            .unwrap())
    }
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let secret = json!({
        "access_token": "old",
        "refresh_token": "oldrt",
        "expires_at_ms": 1,
        "account_uuid": "acct-123",
    });
    let client: Arc<dyn UpstreamClient> = Arc::new(MockUpstream);
    let out = ClaudeCodeChannel.refresh(&client, &secret).await.unwrap();

    assert_eq!(out["access_token"], "new");
    assert_eq!(out["refresh_token"], "newrt");
    assert!(out["expires_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(out["account_uuid"], "acct-123");
}

#[test]
fn prepare_injects_oauth_and_stainless() {
    let secret = json!({ "access_token": "tok-abc" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let ctx = PrepareCtx {
        secret: &secret,
        provider_settings: &settings,
        upstream_model_id: "claude-sonnet-4",
        method: Method::POST,
        path: "/v1/messages",
        query: None,
        headers: &headers,
        body: Bytes::from_static(b"{\"model\":\"claude-sonnet-4\"}"),
    };
    let req = ClaudeCodeChannel.prepare(ctx).unwrap().into_http();

    assert_eq!(
        req.uri().to_string(),
        "https://api.anthropic.com/v1/messages?beta=true"
    );
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer tok-abc"
    );
    assert_eq!(
        req.headers().get("anthropic-beta").unwrap(),
        "oauth-2025-04-20"
    );
    assert_eq!(req.headers().get("x-app").unwrap(), "cli");
    assert_eq!(req.headers().get("x-stainless-lang").unwrap(), "js");
    assert_eq!(
        req.headers().get("x-stainless-package-version").unwrap(),
        "0.81.0"
    );
    assert_eq!(req.headers().get("x-stainless-runtime").unwrap(), "node");
    assert_eq!(
        req.headers().get("user-agent").unwrap(),
        "claude-cli/2.1.112 (external, cli)"
    );
    assert_eq!(
        req.headers().get("x-stainless-runtime-version").unwrap(),
        "v22.20.0"
    );
    assert_eq!(req.headers().get("accept-language").unwrap(), "*");
    assert_eq!(req.headers().get("sec-fetch-mode").unwrap(), "cors");
    assert_eq!(
        req.headers().get("accept-encoding").unwrap(),
        "gzip, deflate"
    );
    assert!(req.headers().get("x-client-request-id").is_none());
    assert!(req.headers().get("x-claude-code-session-id").is_some());
}

#[test]
fn model_query_adds_beta_true_without_duplication() {
    assert_eq!(request::model_query(None), "beta=true");
    assert_eq!(request::model_query(Some("foo=1")), "beta=true&foo=1");
    assert_eq!(
        request::model_query(Some("beta=true&foo=1")),
        "beta=true&foo=1"
    );
}

#[test]
fn anthropic_beta_oauth_first_then_client_deduped() {
    let secret = json!({ "access_token": "tok" });
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        "feat-x,oauth-2025-04-20,feat-y".parse().unwrap(),
    );
    let ctx = PrepareCtx {
        secret: &secret,
        provider_settings: &settings,
        upstream_model_id: "claude-sonnet-4",
        method: Method::POST,
        path: "/v1/messages",
        query: None,
        headers: &headers,
        body: Bytes::from_static(b"{\"messages\":[]}"),
    };
    let req = ClaudeCodeChannel.prepare(ctx).unwrap().into_http();
    assert_eq!(
        req.headers().get("anthropic-beta").unwrap(),
        "oauth-2025-04-20,feat-x,feat-y"
    );
}

#[test]
fn count_tokens_skips_cch_metadata_injection() {
    let secret = json!({ "access_token": "tok", "account_uuid": "acct-1" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let body = Bytes::from_static(b"{\"model\":\"claude-haiku-4-5\",\"messages\":[]}");
    let prepare = |path| {
        let req = ClaudeCodeChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "claude-haiku-4-5",
                method: Method::POST,
                path,
                query: None,
                headers: &headers,
                body: body.clone(),
            })
            .unwrap()
            .into_http();
        serde_json::from_slice::<Value>(req.body()).unwrap()
    };
    let count = prepare("/v1/messages/count_tokens");
    assert!(count.get("metadata").is_none(), "count body: {count}");
    let messages = prepare("/v1/messages");
    assert!(
        messages["metadata"]["user_id"].is_string(),
        "msg body: {messages}"
    );
}

#[tokio::test]
async fn authcode_start_urls() {
    let client: Arc<dyn UpstreamClient> = Arc::new(MockUpstream);
    let claude = ClaudeCodeChannel
        .authcode_start(&client, &json!({}), "", "ST", "CH")
        .await
        .expect("authcode_start ok")
        .expect("claudecode supports authcode");
    let url = &claude.authorize_url;
    assert!(
        url.starts_with("https://claude.ai/oauth/authorize?"),
        "{url}"
    );
    assert!(
        url.contains("client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e"),
        "{url}"
    );
    assert!(url.contains("code_challenge=CH"), "{url}");
    assert!(url.contains("state=ST"), "{url}");
    assert!(url.contains("code_challenge_method=S256"), "{url}");
    assert!(url.contains("redirect_uri="), "{url}");
    assert!(url.contains("scope=user%3Aprofile"), "{url}");
    assert_eq!(claude.redirect_uri, auth::DEFAULT_REDIRECT_URI);
    assert_eq!(
        claude
            .extra
            .as_ref()
            .and_then(|value| value["state"].as_str()),
        Some("ST")
    );

    let gemini = crate::channel::bulletins::geminicli::GeminiCliChannel
        .authcode_start(&client, &json!({}), "", "ST", "CH")
        .await
        .expect("authcode_start ok")
        .expect("geminicli supports authcode");
    let url = &gemini.authorize_url;
    assert!(
        url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"),
        "{url}"
    );
    assert!(url.contains("681255809395-"), "{url}");
    assert!(url.contains("code_challenge=CH"), "{url}");
    assert!(url.contains("state=ST"), "{url}");
    assert!(url.contains("code_challenge_method=S256"), "{url}");
    assert!(url.contains("redirect_uri="), "{url}");
    assert!(url.contains("cloud-platform"), "{url}");
}

fn messages_ctx() -> ShapeCtx<'static> {
    use crate::protocol::{Operation, OperationKey};
    ShapeCtx {
        op: OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        stream: false,
        status: http::StatusCode::OK,
        settings: &serde_json::Value::Null,
    }
}

fn fallback_ctx(settings: &serde_json::Value) -> ShapeCtx<'_> {
    ShapeCtx {
        settings,
        ..messages_ctx()
    }
}

#[test]
fn shape_request_strips_sampling_and_context_1m_keeps_oauth() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        "oauth-2025-04-20,context-1m-2025-08-07".parse().unwrap(),
    );
    let body = Bytes::from_static(
        br#"{"model":"claude-opus-4-8","messages":[],"temperature":0.7,"top_p":0.9,"top_k":40}"#,
    );
    let out = ClaudeCodeChannel.shape_request(body, &mut headers, &messages_ctx());

    let value: Value = serde_json::from_slice(&out).unwrap();
    let map = value.as_object().unwrap();
    assert!(!map.contains_key("temperature"));
    assert!(!map.contains_key("top_p"));
    assert!(!map.contains_key("top_k"));
    assert_eq!(headers.get("anthropic-beta").unwrap(), "oauth-2025-04-20");
}

#[test]
fn shape_request_non_messages_op_is_identity() {
    use crate::protocol::{Operation, OperationKey};
    let mut headers = HeaderMap::new();
    headers.insert("anthropic-beta", "context-1m-2025-08-07".parse().unwrap());
    let body = Bytes::from_static(b"{\"temperature\":0.7}");
    let ctx = ShapeCtx {
        op: OperationKey::provider(Operation::ListModels, Provider::Claude),
        stream: false,
        status: http::StatusCode::OK,
        settings: &serde_json::Value::Null,
    };
    let out = ClaudeCodeChannel.shape_request(body.clone(), &mut headers, &ctx);
    assert_eq!(out, body);
    assert_eq!(
        headers.get("anthropic-beta").unwrap(),
        "context-1m-2025-08-07"
    );
}

#[test]
fn shape_request_injects_fable_fallback_and_keeps_oauth_beta() {
    let mut headers = HeaderMap::new();
    let settings = json!({ "enable_claude_fable_fallback": true });
    headers.insert("anthropic-beta", "oauth-2025-04-20".parse().unwrap());
    let body = Bytes::from_static(br#"{"model":"claude-fable-5","messages":[],"max_tokens":32}"#);
    let out = ClaudeCodeChannel.shape_request(body, &mut headers, &fallback_ctx(&settings));

    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
    assert_eq!(
        headers.get("anthropic-beta").unwrap(),
        "oauth-2025-04-20,server-side-fallback-2026-06-01"
    );
}
