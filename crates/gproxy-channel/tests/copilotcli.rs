#![cfg(feature = "copilotcli")]

//! Copilot CLI: the two tokens a credential holds, the mint that turns one
//! into the other, the editor identity the backend expects, and the seat's
//! metered features.

use gproxy_channel::channel::{
    CredentialContext, CredentialRefresh, DevicePoll, LoginContext, OAuthDeviceCode,
    PrepareContext, QuotaQuery, QuotaScope, QuotaValue,
};
use gproxy_channel::channels::copilotcli::{CopilotCli, QUOTA_SOURCE};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{OneShot, credential, provider, request};

const COPILOT: &str = "tid=short-lived";
const GITHUB: &str = "gho_long_lived";

fn secret() -> Value {
    json!({"access_token": COPILOT, "refresh_token": GITHUB})
}

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    metadata: &Value,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    CopilotCli.prepare(PrepareContext {
        provider: provider("copilotcli", config, base_url),
        credential: credential("oauth", secret, metadata),
        operation: OperationKey {
            operation,
            dialect: Dialect::OpenAiChat,
        },
        request,
        endpoint_override: None,
    })
}

#[test]
fn the_seat_decides_the_origin_and_the_short_lived_token_is_what_inference_sees() {
    let secret = secret();
    let individual = prepare(
        &json!({}),
        None,
        &secret,
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        individual.uri(),
        "https://api.githubcopilot.com/chat/completions"
    );
    assert_eq!(
        individual.headers()["authorization"],
        format!("Bearer {COPILOT}"),
        "the GitHub token never reaches the inference origin"
    );

    let business = prepare(
        &json!({"account_type": "business"}),
        None,
        &secret,
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        business.uri(),
        "https://api.business.githubcopilot.com/chat/completions"
    );

    let recorded = prepare(
        &json!({}),
        None,
        &secret,
        &json!({"account_type": "enterprise"}),
        Operation::ListModels,
        request("/v1/models", Some("limit=2&api_key=leaked")),
    )
    .unwrap();
    assert_eq!(
        recorded.uri(),
        "https://api.enterprise.githubcopilot.com/models?limit=2",
        "the client's own path is replaced and its key never leaves"
    );
    assert_eq!(recorded.method(), http::Method::GET);
}

#[test]
fn a_v3_secret_still_names_the_same_pair() {
    let v3 = json!({"copilot_token": COPILOT, "github_token": GITHUB});
    let prepared = prepare(
        &json!({}),
        None,
        &v3,
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["authorization"],
        format!("Bearer {COPILOT}")
    );
}

#[test]
fn a_credential_that_has_not_been_minted_yet_is_refused() {
    let unminted = json!({"refresh_token": GITHUB});
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &unminted,
            &Value::Null,
            Operation::GenerateContent,
            request("/v1/chat/completions", None)
        ),
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn the_cli_identity_is_the_channels_and_a_client_cannot_forge_it() {
    let secret = secret();
    let mut caller = request("/v1/chat/completions", None);
    for (name, value) in [
        ("copilot-integration-id", "not-the-cli"),
        ("editor-version", "vim/9"),
        ("x-initiator", "agent"),
        ("x-client-machine-id", "someone-elses-machine"),
        ("user-agent", "curl/8"),
    ] {
        caller.headers.insert(name, HeaderValue::from_static(value));
    }
    caller
        .headers
        .insert("x-request-id", HeaderValue::from_static("caller-1"));
    caller.body = HttpBody::Bytes(gproxy_protocol::connection::Bytes::from_static(
        br#"{"model":"gpt-5.4","messages":[{"role":"user","content":"hi"}]}"#,
    ));
    let prepared = prepare(
        &json!({"allowed_headers": ["x-request-id"]}),
        None,
        &secret,
        &Value::Null,
        Operation::GenerateContent,
        caller,
    )
    .unwrap();
    let headers = prepared.headers();
    assert_eq!(headers["copilot-integration-id"], "copilot-developer-cli");
    assert_eq!(headers["editor-version"], "copilot/1.0.61");
    assert_eq!(headers["openai-intent"], "conversation-agent");
    assert_eq!(headers["x-github-api-version"], "2026-06-01");
    assert_eq!(
        headers["user-agent"],
        "copilot/1.0.61 (linux v24.16.0) term/unknown"
    );
    assert_eq!(
        headers["x-initiator"], "user",
        "the conversation says a person is asking, not the client"
    );
    assert_eq!(
        headers["x-request-id"], "caller-1",
        "an allow-listed client header survives"
    );
    let machine = headers["x-client-machine-id"].to_str().unwrap();
    assert_eq!(machine.len(), 36);
    assert_ne!(machine, "someone-elses-machine");
    assert_eq!(headers["x-interaction-id"].to_str().unwrap().len(), 36);
}

#[test]
fn the_machine_is_stable_across_mints_and_the_interaction_is_not() {
    let first = prepare(
        &json!({}),
        None,
        &secret(),
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    // The Copilot token rotates on every refresh; the machine id is derived
    // from the GitHub token, which does not.
    let rotated = json!({"access_token": "tid=another", "refresh_token": GITHUB});
    let second = prepare(
        &json!({}),
        None,
        &rotated,
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        first.headers()["x-client-machine-id"],
        second.headers()["x-client-machine-id"]
    );
    assert_ne!(
        first.headers()["x-interaction-id"],
        second.headers()["x-interaction-id"]
    );
}

#[test]
fn a_conversation_that_has_already_answered_itself_is_the_agent_asking() {
    let mut caller = request("/v1/chat/completions", None);
    caller.body = HttpBody::Bytes(gproxy_protocol::connection::Bytes::from_static(
        br#"{"messages":[{"role":"user","content":"hi"},
             {"role":"assistant","content":"tool"}]}"#,
    ));
    let prepared = prepare(
        &json!({}),
        None,
        &secret(),
        &Value::Null,
        Operation::GenerateContent,
        caller,
    )
    .unwrap();
    assert_eq!(prepared.headers()["x-initiator"], "agent");
}

#[tokio::test]
async fn the_seat_probe_presents_the_github_token_and_skips_unmetered_features() {
    let secret = secret();
    let config = json!({});
    let client = OneShot::new(
        StatusCode::OK,
        json!({
            "copilot_plan": "pro",
            "quota_reset_date": "2026-07-01",
            "quota_snapshots": {
                "chat": {"entitlement": 0, "remaining": 0,
                         "percent_remaining": 100, "unlimited": true},
                "premium_interactions": {"entitlement": 300, "remaining": 270,
                                         "percent_remaining": 90, "unlimited": false},
            }
        })
        .to_string(),
    );
    let snapshot = CopilotCli
        .query(CredentialContext {
            provider: provider("copilotcli", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let (url, headers, _) = client.call(0);
    assert_eq!(url, "https://api.github.com/copilot_internal/user");
    assert_eq!(
        headers["authorization"],
        format!("token {GITHUB}"),
        "GitHub's own surface takes the long-lived token, not the bearer"
    );
    assert_eq!(headers["editor-version"], "vscode/1.95.3");
    assert_eq!(headers["editor-plugin-version"], "copilot-chat/0.43.0");
    assert_eq!(headers["x-github-api-version"], "2025-04-01");

    assert_eq!(
        snapshot.entries.len(),
        1,
        "unlimited features report nothing"
    );
    assert_eq!(snapshot.entries[0].id, "premium_interactions");
    assert_eq!(snapshot.entries[0].source_id, QUOTA_SOURCE);
    assert_eq!(snapshot.entries[0].model_scope, QuotaScope::All);
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.used, Some(30.into()));
    assert_eq!(window.limit, Some(300.into()));
    assert_eq!(window.used_percent, Some(10.into()));
    assert_eq!(window.period_end_ms, Some(1_782_864_000_000));
    // No `QuotaModel`: every metered feature is observed under its own name
    // and a shared source, never charged.
    support::assert_quota_contract(
        None,
        &[],
        &snapshot.entries,
        &["premium_interactions", "chat", "completions"],
    );
}

#[tokio::test]
async fn the_device_login_ends_by_minting_the_first_copilot_token() {
    let config = json!({});
    let start_client = OneShot::new(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "ABCD-1234",
               "verification_uri": "https://github.com/login/device",
               "interval": 5, "expires_in": 900})
        .to_string(),
    );
    let started = CopilotCli
        .start(LoginContext {
            provider: provider("copilotcli", &config, None),
            client: &start_client,
        })
        .await
        .unwrap();
    let (url, headers, body) = start_client.call(0);
    assert_eq!(url, "https://github.com/login/device/code");
    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
    assert_eq!(headers["accept"], "application/json");
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "client_id=Iv1.b507a08c87ecfe98&scope=read%3Auser"
    );
    assert_eq!(started.user_code, "ABCD-1234");

    // GitHub answers a pending authorization with 200 and an error code.
    let pending = OneShot::new(
        StatusCode::OK,
        json!({"error": "authorization_pending"}).to_string(),
    );
    assert!(matches!(
        CopilotCli
            .poll(
                LoginContext {
                    provider: provider("copilotcli", &config, None),
                    client: &pending,
                },
                &started
            )
            .await
            .unwrap(),
        DevicePoll::Pending
    ));

    let granted = TwoShot::new(vec![
        (StatusCode::OK, json!({"access_token": GITHUB}).to_string()),
        (
            StatusCode::OK,
            json!({"token": COPILOT, "expires_at": 1_800_000_000_i64}).to_string(),
        ),
    ]);
    let DevicePoll::Ready(acquired) = CopilotCli
        .poll(
            LoginContext {
                provider: provider("copilotcli", &config, None),
                client: &granted,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    let urls = granted.urls();
    assert_eq!(urls[0], "https://github.com/login/oauth/access_token");
    assert_eq!(
        urls[1], "https://api.github.com/copilot_internal/v2/token",
        "the login spends the GitHub token once so the credential is usable"
    );
    assert_eq!(
        acquired.access_token, COPILOT,
        "the short-lived token is the access token"
    );
    assert_eq!(
        acquired.refresh_token.as_deref(),
        Some(GITHUB),
        "the long-lived GitHub token is what mints the next one"
    );
    assert_eq!(acquired.expires_at_ms, Some(1_800_000_000_000));
}

#[tokio::test]
async fn the_mint_is_the_refresh_and_a_revoked_grant_is_definitive() {
    let config = json!({});
    let secret = secret();

    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
    ] {
        let refused = OneShot::new(status, "revoked".into());
        let outcome = CredentialRefresh::refresh(
            &CopilotCli,
            CredentialContext {
                provider: provider("copilotcli", &config, None),
                credential: credential("oauth", &secret, &Value::Null),
                client: &refused,
            },
        )
        .await;
        assert!(
            matches!(outcome, Err(ChannelError::RefreshRejected(_))),
            "{status}"
        );
    }

    let context = |client| CredentialContext {
        provider: provider("copilotcli", &config, None),
        credential: credential("oauth", &secret, &Value::Null),
        client,
    };

    let transient = OneShot::new(StatusCode::BAD_GATEWAY, "upstream down".into());
    assert!(
        matches!(
            CredentialRefresh::refresh(&CopilotCli, context(&transient)).await,
            Err(ChannelError::UpstreamResponse { .. })
        ),
        "a gateway failure is not a dead credential"
    );

    let minted = OneShot::new(
        StatusCode::OK,
        json!({"token": "tid=fresh", "expires_at": 1_900_000_000_i64}).to_string(),
    );
    let update = CredentialRefresh::refresh(&CopilotCli, context(&minted))
        .await
        .unwrap();
    assert_eq!(update.secret["access_token"], "tid=fresh");
    assert_eq!(
        update.secret["refresh_token"], GITHUB,
        "a full replacement that keeps the long-lived token"
    );
    assert_eq!(update.expires_at_ms, Some(1_900_000_000_000));
    let (url, headers, _) = minted.call(0);
    assert_eq!(url, "https://api.github.com/copilot_internal/v2/token");
    assert_eq!(headers["authorization"], format!("token {GITHUB}"));

    // A v3-shaped secret keeps both name pairs in step.
    let v3 = json!({"copilot_token": COPILOT, "github_token": GITHUB});
    let minted = OneShot::new(
        StatusCode::OK,
        json!({"token": "tid=fresh", "expires_at": 1_900_000_000_i64}).to_string(),
    );
    let update = CredentialRefresh::refresh(
        &CopilotCli,
        CredentialContext {
            provider: provider("copilotcli", &config, None),
            credential: credential("oauth", &v3, &Value::Null),
            client: &minted,
        },
    )
    .await
    .unwrap();
    assert_eq!(update.secret["copilot_token"], "tid=fresh");
    assert_eq!(update.secret["access_token"], "tid=fresh");
    assert_eq!(
        update.secret["copilot_expires_at_ms"],
        1_900_000_000_000_i64
    );
}

#[tokio::test]
async fn a_credential_with_no_github_token_cannot_be_refreshed() {
    let config = json!({});
    let secret = json!({"access_token": COPILOT});
    let client = OneShot::new(StatusCode::OK, "{}".into());
    assert!(matches!(
        CredentialRefresh::refresh(
            &CopilotCli,
            CredentialContext {
                provider: provider("copilotcli", &config, None),
                credential: credential("oauth", &secret, &Value::Null),
                client: &client,
            }
        )
        .await,
        Err(ChannelError::RefreshRejected(_))
    ));
}

#[test]
fn the_descriptor_states_the_one_way_in_and_the_captured_client() {
    let descriptor = CopilotCli.descriptor();
    assert_eq!(descriptor.id, "copilotcli");
    assert_eq!(descriptor.login_modes, [LoginMode::DeviceCode]);
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    for key in ["account_type", "github_api_url", "vscode_version"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
    let connection = CopilotCli
        .default_connection()
        .expect("v3 captured the CLI's ClientHello");
    assert_eq!(connection.backend, gproxy_client::Backend::Wreq);
    let Some(gproxy_client::EmulationConfig::Custom(fingerprint)) = connection.emulation else {
        panic!("a captured fingerprint");
    };
    assert_eq!(fingerprint.alpn, [gproxy_client::Alpn::Http1]);
    assert_eq!(
        fingerprint.curves_list.as_deref(),
        Some("X25519:P-256:P-384")
    );
    assert_eq!(fingerprint.grease, Some(false));
}

// ------------------------------------------------ a two-step scripted client

/// A scripted client that answers a different reply to each call, for the
/// login's device grant followed by its Copilot-token mint.
struct TwoShot {
    replies: Vec<(StatusCode, String)>,
    seen: std::sync::Mutex<Vec<String>>,
}

impl TwoShot {
    fn new(replies: Vec<(StatusCode, String)>) -> Self {
        Self {
            replies,
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn urls(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
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
                body: HttpBody::Bytes(gproxy_protocol::connection::Bytes::from(body)),
            })
        })
    }
}
