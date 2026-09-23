#![cfg(feature = "cline")]

//! Cline: the account token it presents, the envelope it answers in, the
//! catalogue it calls a model list, and the login and refresh behind them.

use gproxy_channel::channel::{
    CredentialContext, CredentialRefresh, DevicePoll, LoginContext, NoState, OAuthDeviceCode,
    OperationContext, PrepareContext, QuotaModel, QuotaQuery, QuotaValue, ResponseView,
    UsageContext, UsageExtractor,
};
use gproxy_channel::channels::cline::{BALANCE_DIMENSION, Cline, PLAN_SOURCE};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

mod support;
use support::{OneShot, credential, provider, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Cline.prepare(PrepareContext {
        provider: provider("cline", config, base_url),
        credential: credential("oauth", secret, &Value::Null),
        operation: OperationKey {
            operation,
            dialect: Dialect::OpenAiChat,
        },
        request,
        endpoint_override: None,
    })
}

fn operation<'a>(
    config: &'a Value,
    secret: &'a Value,
    metadata: &'a Value,
    client: &'a Arc<OneShot>,
    request: WireRequest<HttpBody>,
) -> OperationContext<'a> {
    OperationContext {
        provider: provider("cline", config, None),
        credential: credential("oauth", secret, metadata),
        dialect: Dialect::OpenAiChat,
        request,
        client: client.clone(),
        state: Arc::new(NoState::default()),
        instance_id: Arc::from("test"),
        endpoint_override: None,
    }
}

#[test]
fn an_account_token_is_announced_as_workos_and_a_pasted_key_is_not() {
    let pasted = json!({"api_key": "cl-pasted"});
    let prepared = prepare(
        &json!({}),
        None,
        &pasted,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://api.cline.bot/api/v1/chat/completions"
    );
    assert_eq!(prepared.method(), http::Method::POST);
    assert_eq!(prepared.headers()["authorization"], "Bearer cl-pasted");

    let account = json!({"access_token": "account-jwt", "refresh_token": "r"});
    let prepared = prepare(
        &json!({}),
        Some("https://staging.cline.test/api/v1/"),
        &account,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://staging.cline.test/api/v1/chat/completions"
    );
    assert_eq!(
        prepared.headers()["authorization"],
        "Bearer workos:account-jwt"
    );

    let prefixed = json!({"access_token": "workos:already"});
    let prepared = prepare(
        &json!({}),
        None,
        &prefixed,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["authorization"],
        "Bearer workos:already",
        "the scheme is never announced twice"
    );
}

#[test]
fn the_catalogue_has_a_route_of_its_own_and_the_clients_path_is_not_trusted() {
    let secret = json!({"api_key": "cl"});
    let prepared = prepare(
        &json!({}),
        None,
        &secret,
        Operation::ListModels,
        request("/v1/models", Some("limit=5&api_key=leaked")),
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://api.cline.bot/api/v1/ai/cline/recommended-models?limit=5",
        "the client's own path is replaced and its key never leaves"
    );
    assert_eq!(prepared.method(), http::Method::GET);
}

#[test]
fn a_client_cannot_forge_the_sdk_identity_but_keeps_its_allow_listed_headers() {
    let secret = json!({"api_key": "cl"});
    let mut caller = request("/v1/chat/completions", None);
    for (name, value) in [
        ("http-referer", "https://evil.example"),
        ("x-title", "Not Cline"),
        ("x-client-type", "curl"),
    ] {
        caller.headers.insert(name, HeaderValue::from_static(value));
    }
    caller
        .headers
        .insert("x-request-id", HeaderValue::from_static("caller-1"));
    let prepared = prepare(
        &json!({"allowed_headers": ["x-request-id"]}),
        None,
        &secret,
        Operation::GenerateContent,
        caller,
    )
    .unwrap();
    assert_eq!(prepared.headers()["http-referer"], "https://cline.bot");
    assert_eq!(prepared.headers()["x-title"], "Cline");
    assert_eq!(prepared.headers()["x-client-type"], "cline-sdk");
    assert_eq!(
        prepared.headers()["x-request-id"],
        "caller-1",
        "an allow-listed client header survives"
    );
    assert_eq!(
        prepared.headers()["authorization"],
        "Bearer cl",
        "the caller's own authorization never reaches the upstream"
    );
    assert!(
        prepared.headers().get("anthropic-beta").is_none(),
        "the allow-list narrows everything else"
    );
}

#[test]
fn a_credential_with_neither_token_is_refused() {
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &json!({"user_id": "u"}),
            Operation::GenerateContent,
            request("/v1/chat/completions", None)
        ),
        Err(ChannelError::InvalidCredential)
    ));
}

#[tokio::test]
async fn a_buffered_reply_and_the_catalogue_are_unwrapped_before_the_host_sees_them() {
    let config = json!({});
    let secret = json!({"api_key": "cl"});
    let client = Arc::new(OneShot::new(
        StatusCode::OK,
        json!({"success": true, "data": {"id": "gen-1", "choices": []}}).to_string(),
    ));
    let response = Cline
        .generate_content(operation(
            &config,
            &secret,
            &Value::Null,
            &client,
            request("/v1/chat/completions", None),
        ))
        .await
        .unwrap();
    let HttpBody::Bytes(body) = response.body else {
        panic!("a buffered body");
    };
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["id"], "gen-1");
    assert!(body.get("success").is_none());

    let client = Arc::new(OneShot::new(
        StatusCode::OK,
        json!({"free": [{"id": "a/model"}], "clinePass": [{"id": "b/model"}]}).to_string(),
    ));
    let response = Cline
        .list_models(operation(
            &config,
            &secret,
            &Value::Null,
            &client,
            request("/v1/models", None),
        ))
        .await
        .unwrap();
    let HttpBody::Bytes(body) = response.body else {
        panic!("a buffered body");
    };
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["cline_group"], "free");
    assert_eq!(body["data"][1]["id"], "b/model");
}

#[tokio::test]
async fn a_refusal_is_returned_as_the_upstream_wrote_it() {
    let config = json!({});
    let secret = json!({"api_key": "cl"});
    let client = Arc::new(OneShot::new(
        StatusCode::PAYMENT_REQUIRED,
        json!({"success": false, "error": "out of credits"}).to_string(),
    ));
    let response = Cline
        .generate_content(operation(
            &config,
            &secret,
            &Value::Null,
            &client,
            request("/v1/chat/completions", None),
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::PAYMENT_REQUIRED);
    let HttpBody::Bytes(body) = response.body else {
        panic!("a buffered body");
    };
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["error"], "out of credits");
}

#[test]
fn usage_is_read_through_the_envelope_and_through_a_bare_body_alike() {
    let headers = HeaderMap::new();
    let read = |body: String| {
        Cline
            .extract(UsageContext {
                operation: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::OpenAiChat,
                },
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &headers,
                    body: body.as_bytes(),
                },
            })
            .unwrap()
    };
    let usage = json!({"prompt_tokens": 100, "completion_tokens": 4,
                       "prompt_tokens_details": {"cached_tokens": 30}});
    let wrapped = read(json!({"success": true, "data": {"usage": usage}}).to_string()).unwrap();
    assert_eq!(wrapped.tokens.input_tokens, Some(70));
    assert_eq!(wrapped.tokens.cached_input_tokens, Some(30));
    let bare = read(json!({"usage": usage}).to_string()).unwrap();
    assert_eq!(bare.tokens.input_tokens, Some(70));
    assert!(read(json!({"success": true, "data": {}}).to_string()).is_none());
}

#[test]
fn the_balance_dimension_needs_the_identity_the_login_recorded() {
    let config = json!({});
    let secret = json!({"access_token": "a"});
    let anonymous = Cline.dimensions(
        provider("cline", &config, None),
        credential("oauth", &secret, &Value::Null),
    );
    assert!(anonymous.is_empty(), "no account id, no balance to declare");
    let metadata = json!({"user_id": "user-1"});
    let known = Cline.dimensions(
        provider("cline", &config, None),
        credential("oauth", &secret, &metadata),
    );
    assert_eq!(known[0].id, BALANCE_DIMENSION);
}

/// A client that answers the plan probe and then the balance probe.
struct TwoShot {
    replies: Vec<(StatusCode, String)>,
    seen: std::sync::Mutex<Vec<String>>,
}

impl gproxy_channel::OutboundClient for TwoShot {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> gproxy_protocol::capability::CapabilityFuture<
        'a,
        Result<gproxy_protocol::WireResponse, gproxy_protocol::capability::CapabilityError>,
    > {
        Box::pin(async move {
            let mut seen = self.seen.lock().unwrap();
            let index = seen.len().min(self.replies.len() - 1);
            seen.push(request.uri().to_string());
            let (status, body) = self.replies[index].clone();
            Ok(gproxy_protocol::WireResponse {
                status,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(body)),
            })
        })
    }
}

#[tokio::test]
async fn the_plan_windows_and_the_credit_balance_are_one_snapshot() {
    let config = json!({});
    let secret = json!({"api_key": "cl-key", "access_token": "account-jwt"});
    let metadata = json!({"user_id": "user 1"});
    let client = TwoShot {
        replies: vec![
            (
                StatusCode::OK,
                json!({"success": true, "data": {"limits": [
                    {"type": "five_hour", "percentUsed": 25,
                     "resetsAt": "2030-01-01T05:00:00Z"}
                ]}})
                .to_string(),
            ),
            (
                StatusCode::OK,
                json!({"success": true, "data": {"balance": "12.5"}}).to_string(),
            ),
        ],
        seen: std::sync::Mutex::new(Vec::new()),
    };
    let snapshot = Cline
        .query(CredentialContext {
            provider: provider("cline", &config, None),
            credential: credential("oauth", &secret, &metadata),
            client: &client,
        })
        .await
        .unwrap();
    let urls = client.seen.lock().unwrap().clone();
    assert_eq!(
        urls[0],
        "https://api.cline.bot/api/v1/users/me/plan/usage-limits"
    );
    assert_eq!(
        urls[1], "https://api.cline.bot/api/v1/users/user%201/balance",
        "the account id reaches the URL escaped"
    );
    assert_eq!(snapshot.entries[0].id, "five_hour");
    assert_eq!(snapshot.entries[0].source_id, PLAN_SOURCE);
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.used_percent, Some(25.into()));
    assert_eq!(snapshot.entries[1].id, BALANCE_DIMENSION);
    let QuotaValue::Balance(balance) = &snapshot.entries[1].value else {
        panic!("a balance");
    };
    assert_eq!(balance.remaining, Some("12.5".parse().unwrap()));
    assert_eq!(balance.unit.as_deref(), Some("credits"));
}

#[tokio::test]
async fn a_probe_that_reads_nothing_at_all_reports_the_refusal() {
    let config = json!({});
    let secret = json!({"api_key": "cl-key"});
    let client = OneShot::new(StatusCode::FORBIDDEN, "no plan".into());
    let error = Cline
        .query(CredentialContext {
            provider: provider("cline", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &client,
        })
        .await;
    assert!(matches!(
        error,
        Err(ChannelError::UpstreamResponse { status, .. }) if status == StatusCode::FORBIDDEN
    ));
}

#[tokio::test]
async fn the_login_is_workos_then_cline_and_records_the_account_it_learned() {
    let config = json!({});
    let start_client = OneShot::new(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "ABCD-EFGH",
               "verification_uri": "https://workos.test/device",
               "verification_uri_complete": "https://workos.test/device?code=ABCD-EFGH",
               "interval": 5, "expires_in": 600})
        .to_string(),
    );
    let started = Cline
        .start(LoginContext {
            provider: provider("cline", &config, None),
            client: &start_client,
        })
        .await
        .unwrap();
    let (url, headers, body) = start_client.call(0);
    assert_eq!(
        url,
        "https://api.workos.com/user_management/authorize/device"
    );
    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "client_id=client_01K3A541FN8TA3EPPHTD2325AR"
    );
    assert_eq!(started.user_code, "ABCD-EFGH");

    let pending = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "authorization_pending"}).to_string(),
    );
    assert!(matches!(
        Cline
            .poll(
                LoginContext {
                    provider: provider("cline", &config, None),
                    client: &pending,
                },
                &started
            )
            .await
            .unwrap(),
        DevicePoll::Pending
    ));

    let granted = TwoShot {
        replies: vec![
            (
                StatusCode::OK,
                json!({"access_token": "workos-at", "refresh_token": "workos-rt"}).to_string(),
            ),
            (
                StatusCode::OK,
                json!({"success": true, "data": {
                    "accessToken": "cline-account", "refreshToken": "cline-refresh",
                    "userInfo": {"clineUserId": "user-1", "email": "a@b.test"}
                }})
                .to_string(),
            ),
        ],
        seen: std::sync::Mutex::new(Vec::new()),
    };
    let DevicePoll::Ready(acquired) = Cline
        .poll(
            LoginContext {
                provider: provider("cline", &config, None),
                client: &granted,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    let urls = granted.seen.lock().unwrap().clone();
    assert_eq!(
        urls[0],
        "https://api.workos.com/user_management/authenticate"
    );
    assert_eq!(
        urls[1], "https://api.cline.bot/api/v1/auth/register",
        "WorkOS proves the identity; Cline still mints the account token"
    );
    assert_eq!(acquired.access_token, "cline-account");
    assert_eq!(acquired.refresh_token.as_deref(), Some("cline-refresh"));
    assert_eq!(
        acquired.provider_fields["user_id"], "user-1",
        "the account id the quota probe needs is a fact the login recorded"
    );
    assert_eq!(acquired.provider_fields["email"], "a@b.test");
}

#[tokio::test]
async fn a_refusal_of_the_refresh_token_is_definitive_and_a_rotation_keeps_the_rest() {
    let config = json!({});
    let secret = json!({"access_token": "old", "refresh_token": "rt", "user_id": "user-1"});
    let context = |client| CredentialContext {
        provider: provider("cline", &config, None),
        credential: credential("oauth", &secret, &Value::Null),
        client,
    };

    let refused = OneShot::new(
        StatusCode::OK,
        json!({"success": false, "error": "expired"}).to_string(),
    );
    assert!(matches!(
        CredentialRefresh::refresh(&Cline, context(&refused)).await,
        Err(ChannelError::RefreshRejected(_))
    ));

    let unauthorized = OneShot::new(StatusCode::UNAUTHORIZED, "{}".into());
    assert!(matches!(
        CredentialRefresh::refresh(&Cline, context(&unauthorized)).await,
        Err(ChannelError::RefreshRejected(_))
    ));

    let transient = OneShot::new(StatusCode::BAD_GATEWAY, "upstream down".into());
    assert!(
        matches!(
            CredentialRefresh::refresh(&Cline, context(&transient)).await,
            Err(ChannelError::UpstreamResponse { .. })
        ),
        "a gateway failure is not a dead credential"
    );

    let rotated = OneShot::new(
        StatusCode::OK,
        json!({"success": true, "data": {
            "accessToken": "new-account", "refreshToken": "new-refresh",
            "userInfo": {"clineUserId": "user-1"}
        }})
        .to_string(),
    );
    let update = CredentialRefresh::refresh(&Cline, context(&rotated))
        .await
        .unwrap();
    assert_eq!(update.secret["access_token"], "new-account");
    assert!(update.secret.get("api_key").is_none());
    assert_eq!(update.secret["refresh_token"], "new-refresh");
    assert_eq!(update.secret["user_id"], "user-1");
    let (url, _, body) = rotated.call(0);
    assert_eq!(url, "https://api.cline.bot/api/v1/auth/refresh");
    let sent: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent["refreshToken"], "rt");
    assert_eq!(sent["grantType"], "refresh_token");
}

async fn assert_plan_authorization(secret: &Value, expected: &str) {
    let config = json!({});
    let client = OneShot::new(
        StatusCode::OK,
        json!({"success": true, "data": {"limits": [
            {"type": "five_hour", "percentUsed": 25},
            {"type": "weekly", "percentUsed": 10}
        ]}})
        .to_string(),
    );
    let snapshot = Cline
        .query(CredentialContext {
            provider: provider("cline", &config, None),
            credential: credential("oauth", secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let (url, headers, _) = client.call(0);
    assert_eq!(
        url,
        "https://api.cline.bot/api/v1/users/me/plan/usage-limits"
    );
    assert_eq!(headers["authorization"], expected);
    assert_eq!(snapshot.entries.len(), 2);
}

#[tokio::test]
async fn plan_auth_distinguishes_keys_tokens_and_legacy_copies() {
    for (secret, expected) in [
        (json!({"api_key":"sk-manual"}), "Bearer sk-manual"),
        (json!({"access_token":"old"}), "Bearer workos:old"),
        (
            json!({"api_key":"sk-manual", "access_token":"old"}),
            "Bearer sk-manual",
        ),
        (
            json!({"api_key":"old", "access_token":"old"}),
            "Bearer workos:old",
        ),
        (
            json!({"api_key":" old ", "access_token":"old"}),
            "Bearer workos:old",
        ),
        (
            json!({"api_key":"workos:old", "access_token":"workos:old"}),
            "Bearer workos:old",
        ),
    ] {
        assert_plan_authorization(&secret, expected).await;
    }
}

#[tokio::test]
async fn rotating_oauth_preserves_manual_keys_and_repairs_legacy_copies() {
    let config = json!({});
    for key in [None, Some("sk-manual"), Some("old")] {
        let mut secret = json!({"access_token":"old", "refresh_token":"rt"});
        if let Some(key) = key {
            secret["api_key"] = json!(key);
        }
        // A second rotation checks that the removed legacy copy cannot
        // reappear as a stale API key after the first token replacement.
        for token in ["new", "newer"] {
            let client = OneShot::new(
                StatusCode::OK,
                json!({"success":true,"data":{"accessToken":token}}).to_string(),
            );
            let update = CredentialRefresh::refresh(
                &Cline,
                CredentialContext {
                    provider: provider("cline", &config, None),
                    credential: credential("oauth", &secret, &Value::Null),
                    client: &client,
                },
            )
            .await
            .unwrap();
            // This is the full replacement returned to Core for persistence.
            secret = update.secret;
            assert_eq!(secret["access_token"], token);
            assert_eq!(secret["refresh_token"], "rt");
            let expected = if key == Some("sk-manual") {
                assert_eq!(secret["api_key"], "sk-manual");
                "Bearer sk-manual".to_owned()
            } else {
                assert!(secret.get("api_key").is_none());
                format!("Bearer workos:{token}")
            };
            assert_plan_authorization(&secret, &expected).await;
        }
    }
}

#[test]
fn the_descriptor_offers_both_ways_in() {
    let descriptor = Cline.descriptor();
    assert_eq!(descriptor.id, "cline");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::ApiKey, LoginMode::DeviceCode]
    );
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    for key in ["base_url", "client_id", "token_url", "headers"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
    assert!(
        Cline.default_connection().is_none(),
        "v3 captured no client identity for Cline"
    );
}
