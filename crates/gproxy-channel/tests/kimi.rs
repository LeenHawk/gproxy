#![cfg(feature = "kimi")]

//! Kimi's two products: which one a credential reaches, the CLI identity a
//! subscription must present, and the login that mints it.

use gproxy_channel::channel::{
    CredentialContext, CredentialRefresh, DevicePoll, LoginContext, OAuthDeviceCode,
    PrepareContext, QuotaModel, QuotaQuery, QuotaValue,
};
use gproxy_channel::channels::kimi::{BALANCE_DIMENSION, Kimi, WEEKLY_DIMENSION};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{OneShot, credential, provider, request};

const SUBSCRIPTION: &str = "kimi-access-token";

fn subscription_secret() -> Value {
    json!({
        "access_token": SUBSCRIPTION,
        "refresh_token": "kimi-refresh",
        "device_id": "8f1c0a7e-0000-4000-8000-000000000001",
    })
}

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    auth_kind: &str,
    secret: &Value,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Kimi.prepare(PrepareContext {
        provider: provider("kimi", config, base_url),
        credential: credential(auth_kind, secret, &Value::Null),
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect,
        },
        request,
        endpoint_override: None,
    })
}

#[test]
fn an_api_key_reaches_the_platform_and_a_login_reaches_the_subscription() {
    let key = json!({"api_key": "sk-moonshot"});
    let platform = prepare(
        &json!({}),
        None,
        "api_key",
        &key,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        platform.uri(),
        "https://api.moonshot.cn/v1/chat/completions"
    );
    assert_eq!(platform.headers()["authorization"], "Bearer sk-moonshot");
    assert!(
        platform.headers().get("x-msh-device-id").is_none(),
        "the platform is not the CLI"
    );

    let secret = subscription_secret();
    let code = prepare(
        &json!({}),
        None,
        "oauth",
        &secret,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        code.uri(),
        "https://api.kimi.com/coding/v1/chat/completions",
        "the subscription origin already carries the /v1"
    );
    assert_eq!(
        code.headers()["authorization"],
        format!("Bearer {SUBSCRIPTION}")
    );
    assert_eq!(code.headers()["x-msh-platform"], "kimi_code_cli");
    assert_eq!(
        code.headers()["x-msh-device-id"],
        "8f1c0a7e-0000-4000-8000-000000000001"
    );
    assert_eq!(code.headers()["x-msh-version"], "0.36.1");
    assert_eq!(code.headers()["user-agent"], "kimi_code_cli/0.36.1");
    assert_eq!(
        code.headers()["x-msh-device-name"],
        "unknown",
        "a proxy is not the machine it claims to be unless the operator says so"
    );
}

#[test]
fn the_subscription_answers_claude_messages_on_its_own_wire() {
    let secret = subscription_secret();
    let claude = prepare(
        &json!({}),
        None,
        "oauth",
        &secret,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_eq!(claude.uri(), "https://api.kimi.com/coding/v1/messages");
    assert_eq!(claude.headers()["x-api-key"], SUBSCRIPTION);
    assert_eq!(claude.headers()["anthropic-version"], "2023-06-01");
    assert!(claude.headers().get("authorization").is_none());
}

#[test]
fn the_product_decides_what_the_upstream_can_be_asked() {
    assert_eq!(
        Kimi.native_dialects(
            provider("kimi", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi],
        "unstated, the narrower answer converts safely either way"
    );
    assert_eq!(
        Kimi.native_dialects(
            provider("kimi", &json!({"product": "code"}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    );
    assert_eq!(
        Kimi.native_dialects(
            provider("kimi", &json!({}), Some("https://api.kimi.com/coding/v1")),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude],
        "read off the origin when unstated"
    );
}

#[test]
fn a_client_cannot_forge_the_cli_identity() {
    let secret = subscription_secret();
    let mut caller = request("/v1/chat/completions", None);
    for (name, value) in [
        ("x-msh-device-id", "someone-elses-machine"),
        ("x-msh-platform", "not-the-cli"),
        ("user-agent", "curl/8"),
    ] {
        caller.headers.insert(name, HeaderValue::from_static(value));
    }
    let prepared = prepare(
        &json!({}),
        None,
        "oauth",
        &secret,
        Dialect::OpenAiChat,
        caller,
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["x-msh-device-id"],
        "8f1c0a7e-0000-4000-8000-000000000001"
    );
    assert_eq!(prepared.headers()["x-msh-platform"], "kimi_code_cli");
    assert_eq!(prepared.headers()["user-agent"], "kimi_code_cli/0.36.1");
}

#[test]
fn a_subscription_credential_without_a_device_id_is_refused() {
    let secret = json!({"access_token": SUBSCRIPTION});
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            "oauth",
            &secret,
            Dialect::OpenAiChat,
            request("/v1/chat/completions", None)
        ),
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn the_operator_states_the_machine_the_cli_claims_to_be() {
    let secret = subscription_secret();
    let prepared = prepare(
        &json!({"device_name": "build-01", "device_model": "Mac16,1",
                "os_version": "24.1.0", "cli_version": "0.40.0"}),
        None,
        "oauth",
        &secret,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(prepared.headers()["x-msh-device-name"], "build-01");
    assert_eq!(prepared.headers()["x-msh-device-model"], "Mac16,1");
    assert_eq!(prepared.headers()["x-msh-os-version"], "24.1.0");
    assert_eq!(prepared.headers()["user-agent"], "kimi_code_cli/0.40.0");
}

#[test]
fn moonshots_cache_hit_is_taken_out_of_the_prompt_count() {
    let body = json!({"usage": {
        "prompt_tokens": 500, "completion_tokens": 8, "cached_tokens": 480,
    }})
    .to_string();
    let headers = HeaderMap::new();
    let usage = support::settled(
        &Kimi,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        &headers,
        body.as_bytes(),
    )
    .unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(20));
    assert_eq!(usage.tokens.cached_input_tokens, Some(480));
}

#[tokio::test]
async fn the_quota_surface_follows_the_credential() {
    let secret = subscription_secret();
    let config = json!({});
    let client = OneShot::new(
        StatusCode::OK,
        json!({
            "usage": {"used": "40", "limit": "1000", "resetTime": "2030-01-08T00:00:00Z"},
            "limits": [{
                "name": "Five hour",
                "window": {"duration": 300, "timeUnit": "TIME_UNIT_MINUTE"},
                "detail": {"used": "5", "limit": "100", "resetTime": "2030-01-01T05:00:00Z"}
            }],
        })
        .to_string(),
    );
    let snapshot = Kimi
        .query(CredentialContext {
            provider: provider("kimi", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let (url, headers, _) = client.call(0);
    assert_eq!(url, "https://api.kimi.com/coding/v1/usages");
    assert_eq!(
        headers["x-msh-device-id"], "8f1c0a7e-0000-4000-8000-000000000001",
        "the probe is the same machine as the traffic"
    );
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [WEEKLY_DIMENSION, "five_hour"]
    );
    let QuotaValue::Window(weekly) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(weekly.remaining, Some(960.into()));
    // Named `limits[]` windows beyond the weekly one are observe-only; they
    // are marked `All` though the reply never says which models they cover.
    let declared = Kimi.dimensions(
        provider("kimi", &config, None),
        credential("oauth", &secret, &Value::Null),
    );
    support::assert_quota_contract(Some(&Kimi), &declared, &snapshot.entries, &["five_hour"]);

    let key = json!({"api_key": "sk-moonshot"});
    let balance_client = OneShot::new(
        StatusCode::OK,
        json!({"data": {"available_balance": 12.5}}).to_string(),
    );
    let snapshot = Kimi
        .query(CredentialContext {
            provider: provider("kimi", &config, None),
            credential: credential("api_key", &key, &Value::Null),
            client: &balance_client,
        })
        .await
        .unwrap();
    assert_eq!(
        balance_client.call(0).0,
        "https://api.moonshot.cn/v1/users/me/balance"
    );
    assert_eq!(snapshot.entries[0].id, BALANCE_DIMENSION);
    let declared = Kimi.dimensions(
        provider("kimi", &config, None),
        credential("api_key", &key, &Value::Null),
    );
    support::assert_quota_contract(Some(&Kimi), &declared, &snapshot.entries, &[]);
}

#[test]
fn each_credential_declares_the_dimension_its_product_has() {
    let config = json!({});
    let subscription = subscription_secret();
    let declared = Kimi.dimensions(
        provider("kimi", &config, None),
        credential("oauth", &subscription, &Value::Null),
    );
    assert_eq!(declared[0].id, WEEKLY_DIMENSION);

    let key = json!({"api_key": "sk"});
    let declared = Kimi.dimensions(
        provider("kimi", &config, None),
        credential("api_key", &key, &Value::Null),
    );
    assert_eq!(declared[0].id, BALANCE_DIMENSION);
}

#[tokio::test]
async fn the_device_login_mints_a_machine_and_carries_it_into_the_poll() {
    let config = json!({});
    let start_client = OneShot::new(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "ABCD-EFGH",
               "verification_uri": "https://kimi.com/device",
               "verification_uri_complete": "https://kimi.com/device?code=ABCD-EFGH",
               "interval": 5, "expires_in": 900})
        .to_string(),
    );
    let started = Kimi
        .start(LoginContext {
            provider: provider("kimi", &config, None),
            client: &start_client,
        })
        .await
        .unwrap();
    let (url, headers, body) = start_client.call(0);
    assert_eq!(url, "https://auth.kimi.com/api/oauth/device_authorization");
    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
    assert_eq!(
        String::from_utf8(body).unwrap(),
        "client_id=17e5f671-d194-4dfb-9706-5516cb48c098"
    );
    assert_eq!(started.user_code, "ABCD-EFGH");
    assert_eq!(started.interval_secs, 5);
    let device = started.provider_state["device_id"].as_str().unwrap();
    assert_eq!(device.len(), 36, "a v4 UUID");
    assert_eq!(
        headers["x-msh-device-id"], device,
        "the login announces the machine it just minted"
    );

    // Pending comes back as an error status carrying an OAuth code.
    let pending = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "authorization_pending"}).to_string(),
    );
    assert!(matches!(
        Kimi.poll(
            LoginContext {
                provider: provider("kimi", &config, None),
                client: &pending,
            },
            &started,
        )
        .await
        .unwrap(),
        DevicePoll::Pending
    ));

    let slow = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "slow_down"}).to_string(),
    );
    assert!(matches!(
        Kimi.poll(
            LoginContext {
                provider: provider("kimi", &config, None),
                client: &slow,
            },
            &started,
        )
        .await
        .unwrap(),
        DevicePoll::SlowDown { interval_secs } if interval_secs == 10
    ));

    let granted = OneShot::new(
        StatusCode::OK,
        json!({"access_token": "at", "refresh_token": "rt", "expires_in": 7200}).to_string(),
    );
    let DevicePoll::Ready(acquired) = Kimi
        .poll(
            LoginContext {
                provider: provider("kimi", &config, None),
                client: &granted,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    assert_eq!(acquired.access_token, "at");
    assert_eq!(acquired.refresh_token.as_deref(), Some("rt"));
    assert!(acquired.expires_at_ms.unwrap() > 0);
    assert_eq!(
        acquired.provider_fields["device_id"].as_str(),
        Some(device),
        "the machine travels in the credential so refresh presents the same one"
    );
    let (url, _, body) = granted.call(0);
    assert_eq!(url, "https://auth.kimi.com/api/oauth/token");
    let form = String::from_utf8(body).unwrap();
    assert!(
        form.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"),
        "{form}"
    );
    assert!(form.contains("device_code=dc-1"), "{form}");
}

#[tokio::test]
async fn a_refusal_of_the_refresh_token_is_definitive_and_a_rotation_keeps_the_machine() {
    let config = json!({});
    let secret = subscription_secret();
    let rejected = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant"}).to_string(),
    );
    let error = CredentialRefresh::refresh(
        &Kimi,
        CredentialContext {
            provider: provider("kimi", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &rejected,
        },
    )
    .await;
    assert!(matches!(error, Err(ChannelError::RefreshRejected(_))));

    let transient = OneShot::new(StatusCode::BAD_GATEWAY, "upstream down".into());
    let error = CredentialRefresh::refresh(
        &Kimi,
        CredentialContext {
            provider: provider("kimi", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &transient,
        },
    )
    .await;
    assert!(
        matches!(error, Err(ChannelError::UpstreamResponse { .. })),
        "a gateway failure is not a dead credential"
    );

    let rotated = OneShot::new(
        StatusCode::OK,
        json!({"access_token": "at2", "refresh_token": "rt2", "expires_in": 3600}).to_string(),
    );
    let update = CredentialRefresh::refresh(
        &Kimi,
        CredentialContext {
            provider: provider("kimi", &config, None),
            credential: credential("oauth", &secret, &Value::Null),
            client: &rotated,
        },
    )
    .await
    .unwrap();
    assert_eq!(update.secret["access_token"], "at2");
    assert_eq!(update.secret["refresh_token"], "rt2");
    assert_eq!(
        update.secret["device_id"], "8f1c0a7e-0000-4000-8000-000000000001",
        "a full replacement that keeps what the rotation did not name"
    );
    assert_eq!(
        update.expires_at_ms,
        update.secret["expires_at_ms"].as_i64()
    );
    let (url, headers, body) = rotated.call(0);
    assert_eq!(url, "https://auth.kimi.com/api/oauth/token");
    assert_eq!(
        headers["x-msh-device-id"],
        "8f1c0a7e-0000-4000-8000-000000000001"
    );
    assert!(
        String::from_utf8(body)
            .unwrap()
            .contains("grant_type=refresh_token")
    );
}

#[test]
fn the_descriptor_offers_both_ways_in() {
    let descriptor = Kimi.descriptor();
    assert_eq!(descriptor.id, "kimi");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::ApiKey, LoginMode::DeviceCode]
    );
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    for key in ["product", "oauth_host", "device_name", "cli_version"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
}
