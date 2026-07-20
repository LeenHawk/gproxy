use super::*;
use crate::http::client::ClientError;
use http::{HeaderMap, Method, Response};
use serde_json::json;

struct MockClient {
    body: Vec<u8>,
    seen: std::sync::Mutex<Vec<String>>,
    bodies: std::sync::Mutex<Vec<Bytes>>,
}

#[async_trait::async_trait]
impl UpstreamClient for MockClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
        self.seen.lock().unwrap().push(req.uri().to_string());
        self.bodies.lock().unwrap().push(req.body().clone());
        Ok(Response::new(Bytes::from(self.body.clone())))
    }
}

fn mock(body: Value) -> Arc<MockClient> {
    Arc::new(MockClient {
        body: serde_json::to_vec(&body).unwrap(),
        seen: std::sync::Mutex::new(Vec::new()),
        bodies: std::sync::Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn refresh_dispatches_by_secret_shape() {
    let client = mock(json!({
        "accessToken": "new-access",
        "refreshToken": "new-refresh",
        "profileArn": "arn:aws:kiro:profile/p1",
        "expiresIn": 3600,
    }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let secret = json!({ "access_token": "old", "refresh_token": "old-rt" });
    let out = auth::refresh(&dyn_client, &Value::Null, &secret)
        .await
        .unwrap();
    assert_eq!(out["access_token"], "new-access");
    assert_eq!(out["refresh_token"], "new-refresh");
    assert_eq!(out["profile_arn"], "arn:aws:kiro:profile/p1");
    assert!(out["expires_at_ms"].as_i64().unwrap() > crate::util::time::unix_now() * 1000);
    assert!(client.seen.lock().unwrap()[0].contains("/refreshToken"));

    let client = mock(json!({ "accessToken": "idc-access", "expiresIn": 1800 }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let secret = json!({
        "access_token": "old",
        "refresh_token": "old-rt",
        "client_id": "cid",
        "client_secret": "csecret",
        "region": "eu-west-1",
    });
    let out = auth::refresh(&dyn_client, &Value::Null, &secret)
        .await
        .unwrap();
    assert_eq!(out["access_token"], "idc-access");
    assert_eq!(out["refresh_token"], "old-rt");
    assert!(
        client.seen.lock().unwrap()[0].contains("oidc.eu-west-1.amazonaws.com/token"),
        "SSO-OIDC refresh must hit the region-templated oidc host"
    );
}

#[tokio::test]
async fn sso_authcode_start_registers_and_builds_authorize_url() {
    let client = mock(json!({ "clientId": "reg-cid", "clientSecret": "reg-secret" }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let started = KiroChannel
        .authcode_start(&dyn_client, &json!({}), "", "st-1", "chal-1")
        .await
        .unwrap()
        .expect("kiro has an authcode login");

    assert_eq!(
        client.seen.lock().unwrap()[0],
        "https://oidc.us-east-1.amazonaws.com/client/register"
    );
    let reg: Value = serde_json::from_slice(&client.bodies.lock().unwrap()[0]).unwrap();
    assert_eq!(reg["clientName"], "Kiro-CLI");
    assert_eq!(reg["clientType"], "public");
    assert_eq!(
        reg["grantTypes"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(
        reg["redirectUris"],
        json!(["http://127.0.0.1:1455/oauth/callback"])
    );
    assert_eq!(reg["issuerUrl"], "https://view.awsapps.com/start");
    assert!(
        started
            .authorize_url
            .starts_with("https://oidc.us-east-1.amazonaws.com/authorize?")
    );
    assert!(started.authorize_url.contains("code_challenge_method=S256"));
    assert!(started.authorize_url.contains("client_id=reg-cid"));
    assert_eq!(started.redirect_uri, "http://127.0.0.1:1455/oauth/callback");

    let extra = started.extra.unwrap();
    assert_eq!(extra["client_id"], "reg-cid");
    assert_eq!(extra["client_secret"], "reg-secret");
    assert_eq!(extra["region"], "us-east-1");
    assert_eq!(extra["start_url"], "https://view.awsapps.com/start");
}

#[tokio::test]
async fn authcode_start_rejects_social_and_unknown_methods() {
    let client = mock(json!({ "clientId": "x", "clientSecret": "y" }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    for method in ["social", "external_idp", "builder-id", "totally-bogus"] {
        let err = KiroChannel
            .authcode_start(&dyn_client, &json!({ "auth_method": method }), "", "s", "c")
            .await;
        assert!(err.is_err(), "auth_method={method} must be rejected");
    }
    assert!(
        client.seen.lock().unwrap().is_empty(),
        "rejection must happen before any network call"
    );
}

#[test]
fn request_build() {
    let secret = json!({
        "access_token": "tok",
        "profile_arn": "arn:aws:kiro:profile/abc",
    });
    let settings = json!({});
    let headers = HeaderMap::new();
    let ctx = PrepareCtx {
        secret: &secret,
        provider_settings: &settings,
        op: crate::protocol::OperationKey::content_generation(
            crate::protocol::Operation::GenerateContent,
            crate::protocol::ContentGenerationKind::OpenAiResponses,
        ),
        stream: false,
        upstream_model_id: "claude-sonnet-4-5",
        method: Method::POST,
        path: "/v1/responses",
        query: None,
        headers: &headers,
        body: Bytes::from_static(br#"{"input":"hello kiro"}"#),
    };
    let req = KiroChannel.prepare(ctx).unwrap().into_http();

    assert_eq!(req.uri().to_string(), "https://runtime.us-east-1.kiro.dev/");
    assert_eq!(req.headers().get("authorization").unwrap(), "Bearer tok");
    assert_eq!(
        req.headers().get("x-amz-target").unwrap(),
        "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
    );
    assert_eq!(
        req.headers().get("content-type").unwrap(),
        "application/x-amz-json-1.0"
    );
    assert!(req.headers().get("x-amzn-kiro-agent-mode").is_none());
    assert!(req.headers().get("amz-sdk-invocation-id").is_some());

    let value: Value = serde_json::from_slice(req.body()).unwrap();
    assert_eq!(value["profileArn"], "arn:aws:kiro:profile/abc");
    let user = &value["conversationState"]["currentMessage"]["userInputMessage"];
    assert_eq!(user["content"], "hello kiro");
    assert_eq!(user["modelId"], "claude-sonnet-4.5");
}

#[tokio::test]
async fn kiro_device_start_posts_authorization() {
    let client = mock(json!({
        "deviceCode": "dev-code-1",
        "userCode": "WXYZ-1234",
        "verificationUriComplete": "https://app.kiro.dev/device?user_code=WXYZ-1234",
        "verificationUri": "https://app.kiro.dev/device",
        "intervalInMilliseconds": 5000,
        "expiresInMilliseconds": 900000,
    }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let init = KiroChannel
        .device_start(&dyn_client, &json!({}))
        .await
        .expect("device_start ok");
    assert_eq!(init.device_code, "dev-code-1");
    assert_eq!(init.user_code, "WXYZ-1234");
    assert_eq!(
        init.verification_url,
        "https://app.kiro.dev/device?user_code=WXYZ-1234"
    );
    assert_eq!(init.interval_secs, 5);
    assert_eq!(
        client.seen.lock().unwrap()[0],
        "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/device/authorization"
    );
}

#[tokio::test]
async fn kiro_device_poll_pending_then_authorized() {
    let client = mock(json!({ "status": "authorization_pending" }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let poll = KiroChannel
        .device_poll(&dyn_client, "dev-code-1")
        .await
        .expect("device_poll ok");
    assert!(matches!(poll, DevicePoll::Pending));
    assert_eq!(
        client.seen.lock().unwrap()[0],
        "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/device/poll"
    );

    let client = mock(json!({
        "status": "authorized",
        "accessToken": "at-9",
        "refreshToken": "rt-9",
        "profileArn": "arn:aws:kiro:profile/p9",
        "identityProvider": "Github",
    }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let poll = KiroChannel
        .device_poll(&dyn_client, "dev-code-1")
        .await
        .expect("device_poll ok");
    let secret = match poll {
        DevicePoll::Ready(value) => value,
        other => panic!("expected Ready, got {other:?}"),
    };
    assert_eq!(secret["access_token"], "at-9");
    assert_eq!(secret["refresh_token"], "rt-9");
    assert_eq!(secret["profile_arn"], "arn:aws:kiro:profile/p9");
    assert_eq!(secret["provider"], "Github");
}

#[tokio::test]
async fn kiro_device_poll_denied() {
    let client = mock(json!({ "status": "expired" }));
    let dyn_client: Arc<dyn UpstreamClient> = client.clone();
    let poll = KiroChannel
        .device_poll(&dyn_client, "dev-code-1")
        .await
        .expect("device_poll ok");
    assert!(matches!(poll, DevicePoll::Denied));
}
