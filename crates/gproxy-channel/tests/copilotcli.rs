#![cfg(feature = "copilotcli")]

//! Copilot CLI: the GitHub token a credential holds, the account probe that
//! names its origin, the CLI identity the backend expects, and the seat's
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

const GITHUB: &str = "gho_long_lived";

fn secret() -> Value {
    json!({"access_token": GITHUB})
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
fn the_probed_origin_serves_and_the_github_token_is_the_bearer() {
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
        format!("Bearer {GITHUB}"),
        "the CLI presents the GitHub token itself"
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
        &json!({"api_url": "https://api.enterprise.githubcopilot.com"}),
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

#[tokio::test]
async fn a_credential_from_the_old_app_must_sign_in_again() {
    let config = json!({});
    for old in [
        json!({"access_token": "tid=short-lived", "refresh_token": GITHUB}),
        json!({"copilot_token": "tid=short-lived", "github_token": GITHUB}),
    ] {
        assert!(matches!(
            prepare(
                &config,
                None,
                &old,
                &Value::Null,
                Operation::GenerateContent,
                request("/v1/chat/completions", None)
            ),
            Err(ChannelError::InvalidCredential)
        ));
        assert!(CopilotCli.validate_credential_secret(&old).is_err());
        let refresh = CopilotCli.credential_refresh().unwrap();
        assert!(
            refresh
                .proactive_refresh_reason(credential("oauth", &old, &Value::Null), 0)
                .is_some(),
            "sent to the refresh at once"
        );
        let client = OneShot::new(StatusCode::OK, "{}".into());
        let outcome = refresh
            .refresh(CredentialContext {
                provider: provider("copilotcli", &config, None),
                credential: credential("oauth", &old, &Value::Null),
                client: &client,
            })
            .await;
        assert!(
            matches!(&outcome, Err(ChannelError::RefreshRejected(reason)) if reason.contains("re-login")),
            "refused without asking GitHub"
        );
    }
    let current = secret();
    assert!(CopilotCli.validate_credential_secret(&current).is_ok());
    assert!(
        CopilotCli
            .credential_refresh()
            .unwrap()
            .proactive_refresh_reason(credential("oauth", &current, &Value::Null), 0)
            .is_none(),
        "a GitHub OAuth token has no schedule"
    );
}

#[test]
fn the_cli_identity_is_the_channels_and_a_client_cannot_forge_it() {
    let secret = secret();
    let mut caller = request("/v1/chat/completions", None);
    for (name, value) in [
        ("copilot-integration-id", "not-the-cli"),
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
    assert_eq!(headers["openai-intent"], "conversation-agent");
    for absent in ["editor-version", "x-github-api-version"] {
        assert!(!headers.contains_key(absent), "the CLI sends no {absent}");
    }
    assert_eq!(
        headers["user-agent"],
        "copilot/1.0.77 (linux v24.16.0) term/unknown"
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
fn the_machine_is_stable_for_a_credential_and_the_interaction_is_not() {
    let first = prepare(
        &json!({}),
        None,
        &secret(),
        &Value::Null,
        Operation::GenerateContent,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    let second = prepare(
        &json!({}),
        None,
        &secret(),
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
    assert_eq!(headers["authorization"], format!("Bearer {GITHUB}"));
    assert_eq!(headers["accept"], "application/vnd.github.v3+json");
    for absent in [
        "editor-version",
        "editor-plugin-version",
        "x-github-api-version",
    ] {
        assert!(!headers.contains_key(absent), "the CLI sends no {absent}");
    }

    assert_eq!(
        snapshot.entries.len(),
        1,
        "unlimited features report nothing"
    );
    assert_eq!(snapshot.entries[0].id, "premium_interactions");
    assert_eq!(snapshot.entries[0].source_id, QUOTA_SOURCE);
    assert_eq!(
        snapshot.entries[0].model_scope,
        QuotaScope::Unknown,
        "without catalog billing facts, included models must stay usable"
    );
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
async fn premium_exhaustion_only_covers_models_the_catalog_bills_for() {
    struct Client {
        account: OneShot,
        models: OneShot,
    }
    impl gproxy_channel::OutboundClient for Client {
        fn send<'a>(
            &'a self,
            request: http::Request<HttpBody>,
        ) -> gproxy_protocol::capability::CapabilityFuture<
            'a,
            Result<gproxy_protocol::WireResponse, gproxy_protocol::capability::CapabilityError>,
        > {
            let client = if request.uri().path() == "/models" {
                &self.models
            } else {
                &self.account
            };
            gproxy_channel::OutboundClient::send(client, request)
        }
    }
    let client = Client {
        account: OneShot::new(StatusCode::OK, json!({
            "quota_reset_date":"2099-01-01",
            "quota_snapshots":{"premium_interactions":{"entitlement":300,"remaining":0,"percent_remaining":0}}
        }).to_string()),
        models: OneShot::new(StatusCode::OK, json!({"data":[
            {"id":"premium-model","billing":{"multiplier":1}},
            {"id":"included-model","billing":{"multiplier":0}}
        ]}).to_string()),
    };
    let secret = secret();
    let config = json!({});
    let credential = credential("oauth", &secret, &Value::Null);
    let provider = provider("copilotcli", &config, None);
    let snapshot = CopilotCli
        .query(CredentialContext {
            provider,
            credential,
            client: &client,
        })
        .await
        .unwrap();
    let quota = CopilotCli.quota_model().unwrap();
    let dimensions = quota.dimensions(provider, credential);
    let dimension = quota.classify(&dimensions, &snapshot.entries[0]).unwrap();
    assert!(dimension.scope.matches("premium-model"));
    assert!(!dimension.scope.matches("included-model"));
    assert_eq!(
        client.models.call(0).1["authorization"],
        format!("Bearer {GITHUB}")
    );
}

#[tokio::test]
async fn the_device_login_keeps_the_github_token_and_the_probed_origin() {
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
        "client_id=Ov23ctDVkRmgkPke0Mmm&scope=read%3Auser%2Cread%3Aorg%2Crepo%2Cgist%2Ccodespace"
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
        (
            StatusCode::OK,
            json!({"access_token": GITHUB, "token_type": "bearer"}).to_string(),
        ),
        (
            StatusCode::OK,
            json!({"login": "octocat", "copilot_plan": "business",
                   "endpoints": {"api": "https://api.business.githubcopilot.com"}})
            .to_string(),
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
        urls[1], "https://api.github.com/copilot_internal/user",
        "the login asks who signed in, and mints nothing"
    );
    assert_eq!(acquired.access_token, GITHUB);
    assert_eq!(acquired.refresh_token, None);
    assert_eq!(acquired.expires_at_ms, None);
    assert_eq!(acquired.provider_fields["login"], "octocat");
    assert_eq!(
        acquired.provider_fields["api_url"],
        "https://api.business.githubcopilot.com"
    );
}

#[tokio::test]
async fn the_probe_is_the_refresh_and_a_revoked_token_is_definitive() {
    let config = json!({});
    let secret = secret();

    for status in [StatusCode::UNAUTHORIZED, StatusCode::NOT_FOUND] {
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

    let probed = OneShot::new(
        StatusCode::OK,
        json!({"login": "octocat",
               "endpoints": {"api": "https://api.enterprise.githubcopilot.com"}})
        .to_string(),
    );
    let update = CredentialRefresh::refresh(&CopilotCli, context(&probed))
        .await
        .unwrap();
    assert_eq!(update.secret["access_token"], GITHUB);
    assert_eq!(
        update.secret["provider_fields"]["api_url"],
        "https://api.enterprise.githubcopilot.com"
    );
    assert_eq!(
        update.metadata.as_ref().unwrap()["api_url"],
        "https://api.enterprise.githubcopilot.com"
    );
    assert_eq!(update.expires_at_ms, None);
    let (url, headers, _) = probed.call(0);
    assert_eq!(url, "https://api.github.com/copilot_internal/user");
    assert_eq!(headers["authorization"], format!("Bearer {GITHUB}"));
}

#[tokio::test]
async fn a_credential_with_no_github_token_cannot_be_refreshed() {
    let config = json!({});
    let secret = json!({});
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
    for key in ["account_type", "github_api_url", "scope"] {
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
/// login's device grant followed by its account probe.
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
