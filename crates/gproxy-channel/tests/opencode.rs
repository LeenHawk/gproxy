#![cfg(feature = "opencode")]

//! OpenCode: the tier that decides the origin, the key each surface wants,
//! the conversation header, and the login, refresh and Go usage windows.

use gproxy_channel::channel::{
    CredentialContext, CredentialRefresh, DevicePoll, LoginContext, OAuthDeviceCode,
    PrepareContext, QuotaQuery, QuotaValue, ResponseView, UsageContext, UsageExtractor,
};
use gproxy_channel::channels::opencode::{GO_SOURCE, OpenCode};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{OneShot, credential, provider, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    OpenCode.prepare(PrepareContext {
        provider: provider("opencode", config, base_url),
        credential: credential("oauth", secret, &Value::Null),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: None,
    })
}

fn key() -> Value {
    json!({"api_key": "oc-key"})
}

#[test]
fn the_tier_decides_the_origin_and_the_dialect_decides_the_path() {
    let secret = key();
    let zen = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(zen.uri(), "https://opencode.ai/zen/v1/chat/completions");

    let go = prepare(
        &json!({"tier": "go"}),
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", None),
    )
    .unwrap();
    assert_eq!(go.uri(), "https://opencode.ai/zen/go/v1/responses");

    let models = prepare(
        &json!({}),
        None,
        &secret,
        Operation::ListModels,
        Dialect::OpenAiChat,
        request("/v1/models", None),
    )
    .unwrap();
    assert_eq!(models.uri(), "https://opencode.ai/zen/v1/models");

    let staged = prepare(
        &json!({"tier": "go"}),
        Some("https://staging.opencode.test/v1/"),
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_eq!(staged.uri(), "https://staging.opencode.test/v1/messages");
}

#[test]
fn the_claude_surface_takes_the_same_key_the_way_anthropic_does() {
    let secret = key();
    let claude = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_eq!(claude.headers()["x-api-key"], "oc-key");
    assert_eq!(claude.headers()["anthropic-version"], "2023-06-01");
    assert!(claude.headers().get("authorization").is_none());

    let chat = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(chat.headers()["authorization"], "Bearer oc-key");
    assert!(chat.headers().get("x-api-key").is_none());
}

#[test]
fn an_account_token_authenticates_the_same_way_a_pasted_key_does() {
    let token = json!({"access_token": "oc-account"});
    let prepared = prepare(
        &json!({}),
        None,
        &token,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(prepared.headers()["authorization"], "Bearer oc-account");
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &json!({"refresh_token": "only"}),
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            request("/v1/chat/completions", None)
        ),
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn a_clients_conversation_survives_an_allow_list_and_one_is_minted_otherwise() {
    let secret = key();
    let mut named = request("/v1/chat/completions", None);
    named.headers.insert(
        "x-opencode-session",
        HeaderValue::from_static("client-conversation"),
    );
    let prepared = prepare(
        &json!({"allowed_headers": []}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        named,
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["x-opencode-session"],
        "client-conversation",
        "an allow-list narrows other headers, not the tool's own"
    );

    let first = prepare(
        &json!({}),
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    let minted = first.headers()["x-opencode-session"].to_str().unwrap();
    assert_eq!(minted.len(), 32);
    let second = prepare(
        &json!({}),
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_ne!(minted, second.headers()["x-opencode-session"]);

    let catalogue = prepare(
        &json!({}),
        None,
        &secret,
        Operation::ListModels,
        Dialect::OpenAiChat,
        request("/v1/models", None),
    )
    .unwrap();
    assert!(
        catalogue.headers().get("x-opencode-session").is_none(),
        "a catalogue read is not a conversation"
    );
}

#[test]
fn the_magic_cache_string_is_stripped_and_only_marked_when_the_provider_asks() {
    const TOKEN: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_\
                         49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
    let secret = key();
    let body = |config: Value, dialect| {
        let mut caller = request("/v1/messages", None);
        caller.body = HttpBody::Bytes(gproxy_protocol::connection::Bytes::from(
            json!({"model": "m", "messages": [
                {"role": "user", "content": [{"type": "text", "text": format!("ctx {TOKEN}")}]}
            ]})
            .to_string(),
        ));
        let prepared = prepare(
            &config,
            None,
            &secret,
            Operation::GenerateContent,
            dialect,
            caller,
        )
        .unwrap();
        let HttpBody::Bytes(bytes) = prepared.into_body() else {
            panic!("a buffered body");
        };
        serde_json::from_slice::<Value>(&bytes).unwrap()
    };

    let off = body(json!({}), Dialect::Claude);
    let text = off["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("GPROXY_MAGIC_STRING"), "always stripped");
    assert!(off["messages"][0]["content"][0]["cache_control"].is_null());

    let on = body(json!({"enable_claude_magic_cache": true}), Dialect::Claude);
    assert_eq!(
        on["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert_eq!(
        on["messages"][0]["content"][0]["cache_control"]["ttl"],
        "5m"
    );
}

#[test]
fn usage_is_read_from_whichever_shape_the_surface_answered_in() {
    let headers = HeaderMap::new();
    let read = |dialect, body: String| {
        OpenCode
            .extract(UsageContext {
                operation: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect,
                },
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &headers,
                    body: body.as_bytes(),
                },
            })
            .unwrap()
            .unwrap()
    };
    let chat = read(
        Dialect::OpenAiChat,
        json!({"usage": {"prompt_tokens": 100, "completion_tokens": 4,
                         "prompt_tokens_details": {"cached_tokens": 40}}})
        .to_string(),
    );
    assert_eq!(chat.tokens.input_tokens, Some(60));
    let claude = read(
        Dialect::Claude,
        json!({"usage": {"input_tokens": 100, "output_tokens": 4,
                         "cache_read_input_tokens": 40}})
        .to_string(),
    );
    assert_eq!(
        claude.tokens.input_tokens,
        Some(100),
        "Claude already excludes its cache counters"
    );
}

#[tokio::test]
async fn only_the_go_tier_has_usage_windows_to_report() {
    let secret = key();
    let zen = OneShot::new(StatusCode::OK, "{}".into());
    assert!(matches!(
        OpenCode
            .query(CredentialContext {
                provider: provider("opencode", &json!({}), None),
                credential: credential("api_key", &secret, &Value::Null),
                client: &zen,
            })
            .await,
        Err(ChannelError::UnsupportedService)
    ));

    let config = json!({"tier": "go"});
    let client = OneShot::new(
        StatusCode::OK,
        json!({"usage": {
            "rolling": {"status": "ok", "percent": 12.5,
                        "resetsAt": "2030-01-01T05:00:00Z"},
            "weekly": {"status": "rate-limited", "percent": 100,
                       "resetsAt": "2030-01-05T00:00:00Z"},
            "monthly": {"status": "ok", "percent": 0,
                        "resetsAt": "2030-02-01T00:00:00Z"},
        }})
        .to_string(),
    );
    let snapshot = OpenCode
        .query(CredentialContext {
            provider: provider("opencode", &config, None),
            credential: credential("api_key", &secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let (url, headers, _) = client.call(0);
    assert_eq!(url, "https://opencode.ai/zen/go/v1/usage");
    assert_eq!(headers["authorization"], "Bearer oc-key");
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["rolling", "weekly", "monthly"]
    );
    assert!(snapshot.entries.iter().all(|e| e.source_id == GO_SOURCE));
    let QuotaValue::Window(rolling) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(rolling.used_percent, Some("12.5".parse().unwrap()));

    // An origin pointed at the Go tier states it as plainly as the key would.
    let by_origin = OneShot::new(StatusCode::FORBIDDEN, "no".into());
    assert!(matches!(
        OpenCode
            .query(CredentialContext {
                provider: provider(
                    "opencode",
                    &json!({}),
                    Some("https://opencode.ai/zen/go/v1")
                ),
                credential: credential("api_key", &secret, &Value::Null),
                client: &by_origin,
            })
            .await,
        Err(ChannelError::UpstreamResponse { .. })
    ));
}

#[tokio::test]
async fn the_console_login_records_the_console_it_used() {
    let config = json!({});
    let start_client = OneShot::new(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "WXYZ",
               "verification_uri": "/device",
               "verification_uri_complete": "/device?code=WXYZ",
               "interval": 5, "expires_in": 600})
        .to_string(),
    );
    let started = OpenCode
        .start(LoginContext {
            provider: provider("opencode", &config, None),
            client: &start_client,
        })
        .await
        .unwrap();
    let (url, headers, body) = start_client.call(0);
    assert_eq!(url, "https://console.opencode.ai/auth/device/code");
    assert_eq!(headers["content-type"], "application/json");
    let sent: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent["client_id"], "opencode-cli");
    assert_eq!(
        started.verification_uri, "https://console.opencode.ai/device",
        "a relative verification uri hangs off the console"
    );

    let denied = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "access_denied"}).to_string(),
    );
    assert!(matches!(
        OpenCode
            .poll(
                LoginContext {
                    provider: provider("opencode", &config, None),
                    client: &denied,
                },
                &started
            )
            .await
            .unwrap(),
        DevicePoll::Denied
    ));

    let granted = OneShot::new(
        StatusCode::OK,
        json!({"access_token": "at", "refresh_token": "rt", "expires_in": 3600}).to_string(),
    );
    let DevicePoll::Ready(acquired) = OpenCode
        .poll(
            LoginContext {
                provider: provider("opencode", &config, None),
                client: &granted,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    assert_eq!(
        granted.call(0).0,
        "https://console.opencode.ai/auth/device/token"
    );
    assert_eq!(acquired.access_token, "at");
    assert_eq!(acquired.refresh_token.as_deref(), Some("rt"));
    assert!(acquired.expires_at_ms.unwrap() > 0);
    assert_eq!(
        acquired.provider_fields["console_base_url"], "https://console.opencode.ai",
        "the refresh has to return to the console the login used"
    );
}

#[tokio::test]
async fn a_refusal_of_the_refresh_token_is_definitive_and_a_rotation_keeps_the_console() {
    let config = json!({});
    let secret = json!({
        "access_token": "old", "refresh_token": "rt",
        "console_base_url": "https://console.example",
    });
    let context = |client| CredentialContext {
        provider: provider("opencode", &config, None),
        credential: credential("oauth", &secret, &Value::Null),
        client,
    };

    let rejected = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant"}).to_string(),
    );
    assert!(matches!(
        CredentialRefresh::refresh(&OpenCode, context(&rejected)).await,
        Err(ChannelError::RefreshRejected(_))
    ));

    let transient = OneShot::new(StatusCode::BAD_GATEWAY, "upstream down".into());
    assert!(
        matches!(
            CredentialRefresh::refresh(&OpenCode, context(&transient)).await,
            Err(ChannelError::UpstreamResponse { .. })
        ),
        "a gateway failure is not a dead credential"
    );

    let rotated = OneShot::new(
        StatusCode::OK,
        json!({"access_token": "at2", "refresh_token": "rt2", "expires_in": 60}).to_string(),
    );
    let update = CredentialRefresh::refresh(&OpenCode, context(&rotated))
        .await
        .unwrap();
    assert_eq!(update.secret["access_token"], "at2");
    assert_eq!(update.secret["api_key"], "at2");
    assert_eq!(update.secret["refresh_token"], "rt2");
    assert_eq!(update.secret["console_base_url"], "https://console.example");
    assert_eq!(
        update.expires_at_ms,
        update.secret["expires_at_ms"].as_i64()
    );
    assert_eq!(
        rotated.call(0).0,
        "https://console.example/auth/device/token",
        "the credential's own console, not the default"
    );

    // An upstream that states no lifetime leaves no stale expiry behind.
    let silent = OneShot::new(StatusCode::OK, json!({"access_token": "at3"}).to_string());
    let update = CredentialRefresh::refresh(&OpenCode, context(&silent))
        .await
        .unwrap();
    assert_eq!(update.expires_at_ms, None);
    assert!(update.secret.get("expires_at_ms").is_none());
}

#[test]
fn the_descriptor_offers_both_ways_in_and_no_captured_client() {
    let descriptor = OpenCode.descriptor();
    assert_eq!(descriptor.id, "opencode");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::ApiKey, LoginMode::DeviceCode]
    );
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    for key in ["tier", "console_base_url", "enable_openai_magic_cache"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
    assert!(
        OpenCode.default_connection().is_none(),
        "v3 captured no client identity for OpenCode"
    );
    assert_eq!(
        OpenCode.native_dialects(
            provider("opencode", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    );
}
