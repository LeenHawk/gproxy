use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use bytes::Bytes;
use http::{Request, Response};
use serde_json::{Value, json};

use super::super::{auth, token};
use crate::channel::login::DevicePoll;
use crate::channel::oauth::TokenResponse;
use crate::http::client::{ClientError, UpstreamClient};

struct CapturingUpstream {
    seen: Mutex<Option<Request<Bytes>>>,
    body: Bytes,
}

#[async_trait::async_trait]
impl UpstreamClient for CapturingUpstream {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        *self.seen.lock().unwrap() = Some(request);
        Ok(Response::builder()
            .status(200)
            .body(self.body.clone())
            .unwrap())
    }
}

struct QueueUpstream {
    responses: Mutex<Vec<(u16, Vec<u8>)>>,
    seen: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl UpstreamClient for QueueUpstream {
    async fn send(&self, request: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        self.seen.lock().unwrap().push(request.uri().to_string());
        let (status, body) = self.responses.lock().unwrap().remove(0);
        Ok(Response::builder()
            .status(status)
            .body(Bytes::from(body))
            .unwrap())
    }
}

fn queue(responses: Vec<(u16, Value)>) -> Arc<QueueUpstream> {
    Arc::new(QueueUpstream {
        responses: Mutex::new(
            responses
                .into_iter()
                .map(|(status, value)| (status, serde_json::to_vec(&value).unwrap()))
                .collect(),
        ),
        seen: Mutex::new(Vec::new()),
    })
}

fn id_token(account_id: &str) -> String {
    let payload = json!({
        "https://api.openai.com/auth": { "chatgpt_account_id": account_id }
    });
    format!(
        "header.{}.signature",
        B64URL.encode(serde_json::to_vec(&payload).unwrap())
    )
}

#[tokio::test]
async fn authcode_exchange_builds_request_and_maps_secret() {
    let id_token = id_token("acct-77");
    let response = json!({
        "access_token": "at-new",
        "refresh_token": "rt-new",
        "id_token": id_token,
        "expires_in": 3600,
        "token_type": "Bearer",
    });
    let upstream = Arc::new(CapturingUpstream {
        seen: Mutex::new(None),
        body: Bytes::from(serde_json::to_vec(&response).unwrap()),
    });
    let client: Arc<dyn UpstreamClient> = upstream.clone();

    let secret = auth::authcode_exchange(
        &client,
        "the-code",
        "the-verifier",
        "http://localhost:1455/auth/callback",
    )
    .await
    .expect("exchange ok");

    let request = upstream.seen.lock().unwrap().take().expect("a request");
    assert_eq!(request.method(), http::Method::POST);
    assert_eq!(request.uri(), "https://auth.openai.com/oauth/token");
    let body = String::from_utf8(request.body().to_vec()).unwrap();
    assert!(body.contains("grant_type=authorization_code"));
    assert!(body.contains("code=the-code"));
    assert!(body.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"));
    assert!(body.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
    assert!(body.contains("code_verifier=the-verifier"));
    assert_eq!(secret["access_token"], "at-new");
    assert_eq!(secret["refresh_token"], "rt-new");
    assert_eq!(secret["account_id"], "acct-77");
    assert_eq!(secret["id_token"], id_token);
    assert!(secret["expires_at_ms"].as_i64().unwrap() > crate::util::time::unix_now() * 1000);
}

#[test]
fn decodes_id_token_claims_and_maps_login_secret() {
    let payload = json!({
        "https://api.openai.com/auth": { "chatgpt_account_id": "acct-xyz" },
        "email": "u@example.com"
    });
    let jwt = format!(
        "h.{}.s",
        B64URL.encode(serde_json::to_vec(&payload).unwrap())
    );
    assert_eq!(
        token::account_id_from_id_token(&jwt).as_deref(),
        Some("acct-xyz")
    );
    let secret = token::secret_from_login(TokenResponse {
        access_token: Some("access".into()),
        refresh_token: None,
        expires_in: Some(3600),
        id_token: Some(jwt),
    })
    .unwrap();
    assert_eq!(secret["user_email"], "u@example.com");

    let no_claim = B64URL.encode(br#"{"email":"x"}"#);
    assert_eq!(
        token::account_id_from_id_token(&format!("h.{no_claim}.s")),
        None
    );
    assert_eq!(token::account_id_from_id_token("not-a-jwt"), None);
}

#[test]
fn refresh_predicate_preserves_unknown_expiry_behavior() {
    assert!(token::needs_refresh(&json!({})));
    assert!(!token::needs_refresh(&json!({ "access_token": "access" })));
    assert!(!token::needs_refresh(&json!({
        "access_token": "access",
        "expires_at_ms": crate::util::time::unix_now() * 1000 + 600_000
    })));
    assert!(token::needs_refresh(&json!({
        "access_token": "access",
        "expires_at_ms": crate::util::time::unix_now() * 1000 + 1_000
    })));
}

#[tokio::test]
async fn refresh_rotates_returned_tokens_and_preserves_other_fields() {
    let upstream = queue(vec![(
        200,
        json!({ "access_token": "new-access", "expires_in": 3600 }),
    )]);
    let client: Arc<dyn UpstreamClient> = upstream;
    let old = json!({
        "access_token": "old-access",
        "refresh_token": "old-refresh",
        "account_id": "acct-old",
        "custom": "preserved"
    });
    let refreshed = token::refresh(&client, &old).await.unwrap();
    assert_eq!(refreshed["access_token"], "new-access");
    assert_eq!(refreshed["refresh_token"], "old-refresh");
    assert_eq!(refreshed["account_id"], "acct-old");
    assert_eq!(refreshed["custom"], "preserved");
}

#[tokio::test]
async fn device_start_requests_user_code() {
    let upstream = queue(vec![(
        200,
        json!({ "device_auth_id": "dev-1", "user_code": "WXYZ-1234", "interval": "7" }),
    )]);
    let client: Arc<dyn UpstreamClient> = upstream.clone();
    let init = auth::device_start(&client).await.expect("device_start ok");
    assert_eq!(
        upstream.seen.lock().unwrap()[0],
        "https://auth.openai.com/api/accounts/deviceauth/usercode"
    );
    assert_eq!(init.user_code, "WXYZ-1234");
    assert_eq!(
        init.verification_url,
        "https://auth.openai.com/codex/device"
    );
    assert_eq!(init.interval_secs, 7);
    let state: Value = serde_json::from_str(&init.device_code).unwrap();
    assert_eq!(state["device_auth_id"], "dev-1");
    assert_eq!(state["user_code"], "WXYZ-1234");
}

#[tokio::test]
async fn device_poll_pending_then_authorized() {
    let device_code = serde_json::to_string(&json!({
        "device_auth_id": "dev-1", "user_code": "WXYZ-1234"
    }))
    .unwrap();
    let upstream = queue(vec![(403, json!({}))]);
    let client: Arc<dyn UpstreamClient> = upstream.clone();
    assert!(matches!(
        auth::device_poll(&client, &device_code).await.unwrap(),
        DevicePoll::Pending
    ));
    assert_eq!(
        upstream.seen.lock().unwrap()[0],
        "https://auth.openai.com/api/accounts/deviceauth/token"
    );

    let id_token = id_token("acct-9");
    let upstream = queue(vec![
        (
            200,
            json!({ "authorization_code": "auth-code", "code_verifier": "ver" }),
        ),
        (
            200,
            json!({ "access_token": "at-d", "refresh_token": "rt-d", "id_token": id_token, "expires_in": 3600 }),
        ),
    ]);
    let client: Arc<dyn UpstreamClient> = upstream.clone();
    let DevicePoll::Ready(secret) = auth::device_poll(&client, &device_code).await.unwrap() else {
        panic!("expected ready device poll");
    };
    assert_eq!(secret["access_token"], "at-d");
    assert_eq!(secret["refresh_token"], "rt-d");
    assert_eq!(secret["account_id"], "acct-9");
    assert_eq!(
        upstream.seen.lock().unwrap().as_slice(),
        [
            "https://auth.openai.com/api/accounts/deviceauth/token",
            "https://auth.openai.com/oauth/token"
        ]
    );
}
