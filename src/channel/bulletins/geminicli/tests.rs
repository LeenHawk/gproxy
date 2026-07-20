use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::GeminiCliChannel;
use crate::channel::{Channel, ChannelError, ChannelLogin, PrepareCtx, ShapeCtx};
use crate::http::client::UpstreamClient;

fn ctx_for<'a>(
    secret: &'a Value,
    settings: &'a Value,
    headers: &'a HeaderMap,
    path: &'a str,
    body: &'static [u8],
) -> PrepareCtx<'a> {
    PrepareCtx {
        secret,
        provider_settings: settings,
        op: crate::protocol::OperationKey::content_generation(
            crate::protocol::Operation::GenerateContent,
            crate::protocol::ContentGenerationKind::GeminiGenerateContent,
        ),
        stream: false,
        upstream_model_id: "gemini-2.5-pro",
        method: Method::POST,
        path,
        query: None,
        headers,
        body: Bytes::from_static(body),
    }
}

#[test]
fn prepare_wraps_envelope_and_builds_v1internal() {
    let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
    let settings = json!({});
    let headers = HeaderMap::new();

    let ctx = ctx_for(
        &secret,
        &settings,
        &headers,
        "/v1beta/models/gemini-2.5-pro:generateContent",
        br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
    );
    let req = GeminiCliChannel.prepare(ctx).unwrap().into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://cloudcode-pa.googleapis.com/v1internal:generateContent"
    );
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer tok-abc"
    );
    assert_eq!(
        req.headers().get("user-agent").unwrap(),
        "GeminiCLI-tui/0.46.0/gemini-2.5-pro (linux; x64; terminal) google-api-nodejs-client/9.15.1"
    );

    let value: Value = serde_json::from_slice(req.body()).unwrap();
    assert_eq!(value["model"], "gemini-2.5-pro");
    assert_eq!(value["project"], "proj");
    assert!(
        value["user_prompt_id"]
            .as_str()
            .is_some_and(|id| id.len() == 32)
    );
    assert_eq!(value["request"]["contents"][0]["parts"][0]["text"], "hi");

    let ctx = ctx_for(
        &secret,
        &settings,
        &headers,
        "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
        br#"{"contents":[]}"#,
    );
    let req = GeminiCliChannel.prepare(ctx).unwrap().into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
}

#[test]
fn list_models_builds_retrieve_user_quota() {
    let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let ctx = PrepareCtx {
        secret: &secret,
        provider_settings: &settings,
        op: crate::protocol::OperationKey::provider(
            crate::protocol::Operation::ListModels,
            crate::protocol::Provider::Gemini,
        ),
        stream: false,
        upstream_model_id: "",
        method: Method::GET,
        path: "/v1beta/models",
        query: None,
        headers: &headers,
        body: Bytes::new(),
    };
    let req = GeminiCliChannel.prepare(ctx).unwrap().into_http();
    assert_eq!(req.method(), Method::POST);
    assert_eq!(
        req.uri().to_string(),
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"
    );
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer tok-abc"
    );
    let value: Value = serde_json::from_slice(req.body()).unwrap();
    assert_eq!(value, json!({ "project": "proj" }));
}

#[test]
fn shape_response_reshapes_quota_to_model_list() {
    let shape = ShapeCtx {
        op: crate::protocol::OperationKey::provider(
            crate::protocol::Operation::ListModels,
            crate::protocol::Provider::Gemini,
        ),
        stream: false,
        status: http::StatusCode::OK,
        settings: &Value::Null,
    };
    let out = GeminiCliChannel.shape_response(
        Bytes::from(
            json!({"buckets": [
                {"modelId": "gemini-2.5-pro", "tokenType": "REQUESTS"}
            ]})
            .to_string(),
        ),
        &shape,
    );
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value["models"][0]["name"], "models/gemini-2.5-pro");
}

#[test]
fn normalize_unwraps_and_needs_refresh_expiry() {
    let shape = ShapeCtx {
        op: crate::protocol::OperationKey::content_generation(
            crate::protocol::Operation::GenerateContent,
            crate::protocol::ContentGenerationKind::GeminiGenerateContent,
        ),
        stream: false,
        status: http::StatusCode::OK,
        settings: &Value::Null,
    };
    let out = GeminiCliChannel.shape_response(
        Bytes::from_static(br#"{"response":{"candidates":[]}}"#),
        &shape,
    );
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(value, json!({"candidates": []}));

    let out = GeminiCliChannel.shape_response(
        Bytes::from(
            json!({"response": {
                "candidates": [{"citationMetadata": {"citations": [{"uri": "x"}]}}]
            }})
            .to_string(),
        ),
        &shape,
    );
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert!(value.get("response").is_none());
    assert_eq!(
        value["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
        "x"
    );

    let mut req_headers = HeaderMap::new();
    let out = GeminiCliChannel.shape_request(
        Bytes::from(
            json!({"generationConfig": {"maxOutputTokens": 8, "temperature": 0.5}}).to_string(),
        ),
        &mut req_headers,
        &shape,
    );
    let value: Value = serde_json::from_slice(&out).unwrap();
    assert!(
        value["generationConfig"]
            .as_object()
            .unwrap()
            .get("maxOutputTokens")
            .is_none()
    );
    assert_eq!(value["generationConfig"]["temperature"], 0.5);

    let settings = json!({});
    let headers = HeaderMap::new();
    let no_project = json!({ "access_token": "t" });
    let error = GeminiCliChannel
        .prepare(ctx_for(
            &no_project,
            &settings,
            &headers,
            "/x:generateContent",
            b"{}",
        ))
        .unwrap_err();
    assert!(
        matches!(error, ChannelError::InvalidCredential(message) if message.contains("project_id"))
    );

    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    assert!(GeminiCliChannel.needs_refresh(&json!({})));
    assert!(GeminiCliChannel.needs_refresh(&json!({
        "access_token": "t", "expires_at_ms": now_ms + 10_000,
    })));
    assert!(!GeminiCliChannel.needs_refresh(&json!({
        "access_token": "t", "expires_at_ms": now_ms + 600_000,
    })));
}

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

#[tokio::test]
async fn authcode_start_selects_redirect_by_code_only() {
    let client: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);

    let start = GeminiCliChannel
        .authcode_start(&client, &json!({}), "", "ST", "CH")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(start.redirect_uri, "https://codeassist.google.com/authcode");
    assert!(
        start
            .authorize_url
            .contains("redirect_uri=https%3A%2F%2Fcodeassist.google.com%2Fauthcode")
    );

    let start = GeminiCliChannel
        .authcode_start(&client, &json!({ "code_only": true }), "", "ST", "CH")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(start.redirect_uri, "https://codeassist.google.com/authcode");

    let start = GeminiCliChannel
        .authcode_start(&client, &json!({ "code_only": false }), "", "ST", "CH")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(start.redirect_uri, "http://127.0.0.1:1455/oauth2callback");
    assert!(
        start
            .authorize_url
            .contains("127.0.0.1%3A1455%2Foauth2callback")
    );

    let start = GeminiCliChannel
        .authcode_start(
            &client,
            &json!({ "code_only": true }),
            "http://127.0.0.1:9999/cb",
            "ST",
            "CH",
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(start.redirect_uri, "http://127.0.0.1:9999/cb");
}
