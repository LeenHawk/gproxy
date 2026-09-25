#![cfg(feature = "opencode")]

//! OpenCode: the tier that decides the origin, the key each surface wants,
//! the conversation header, and the login, refresh and Go usage windows.

use gproxy_channel::channel::{
    CredentialContext, CredentialRefresh, DevicePoll, LoginContext, OAuthDeviceCode,
    PrepareContext, QuotaValue, ResponseView, UsageContext, UsageExtractor,
};
use gproxy_channel::channels::opencode::{GO_SOURCE, OpenCode};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode, OutboundClient};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;

mod support;
use support::{OneShot, credential, provider, request};

/// Replies handed out in order, for a flow that makes several calls.
struct Script {
    replies: Mutex<VecDeque<(StatusCode, Value)>>,
    seen: Mutex<Vec<(String, HeaderMap)>>,
}

impl Script {
    fn new(replies: Vec<(StatusCode, Value)>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn sent(&self) -> Vec<(String, HeaderMap)> {
        self.seen.lock().unwrap().clone()
    }
}

impl OutboundClient for Script {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, _) = request.into_parts();
            self.seen
                .lock()
                .unwrap()
                .push((parts.uri.to_string(), parts.headers));
            let (status, body) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected upstream call");
            Ok(WireResponse {
                status,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(body.to_string())),
            })
        })
    }
}

fn prepare(
    channel: OpenCode,
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    channel.prepare(PrepareContext {
        provider: provider(channel.id(), config, base_url),
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
fn the_channel_decides_the_origin_and_the_dialect_decides_the_path() {
    let secret = key();
    let zen = prepare(
        OpenCode::ZEN,
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
        OpenCode::GO,
        &json!({}),
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", None),
    )
    .unwrap();
    assert_eq!(go.uri(), "https://opencode.ai/zen/go/v1/responses");

    let models = prepare(
        OpenCode::ZEN,
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
        OpenCode::GO,
        &json!({}),
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
        OpenCode::ZEN,
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
        OpenCode::ZEN,
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
        OpenCode::ZEN,
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
            OpenCode::ZEN,
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
        OpenCode::ZEN,
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
        OpenCode::ZEN,
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
        OpenCode::ZEN,
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
        OpenCode::ZEN,
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
            OpenCode::ZEN,
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
        OpenCode::ZEN
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
async fn only_the_go_channel_has_usage_windows_to_report() {
    assert!(OpenCode::ZEN.quota_query().is_none());
    assert!(!OpenCode::ZEN.descriptor().capabilities.quota_query);

    let secret = key();
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
    let snapshot = OpenCode::GO
        .quota_query()
        .expect("Go reports usage")
        .query(CredentialContext {
            provider: provider("opencodego", &json!({}), None),
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
}

#[tokio::test]
async fn the_console_login_records_the_console_and_the_account_it_used() {
    let config = json!({});
    // The Console's own reply: both verification URIs are origin-rooted.
    let start_client = OneShot::new(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "WXYZ",
               "verification_uri": "/console/device",
               "verification_uri_complete":
                   "/console/device?user_code=WXYZ&client_id=opencode-cli",
               "interval": 5, "expires_in": 600})
        .to_string(),
    );
    let started = OpenCode::ZEN
        .start(LoginContext {
            provider: provider("opencodezen", &config, None),
            client: &start_client,
        })
        .await
        .unwrap();
    let (url, headers, body) = start_client.call(0);
    assert_eq!(url, "https://opencode.ai/console/auth/device/code");
    assert_eq!(headers["content-type"], "application/json");
    let sent: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(sent["client_id"], "opencode-cli");
    assert_eq!(
        started.verification_uri, "https://opencode.ai/console/device",
        "a rooted verification uri hangs off the origin, not the console path"
    );
    assert_eq!(
        started.verification_uri_complete.as_deref(),
        Some("https://opencode.ai/console/device?user_code=WXYZ&client_id=opencode-cli")
    );

    let denied = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "access_denied"}).to_string(),
    );
    assert!(matches!(
        OpenCode::ZEN
            .poll(
                LoginContext {
                    provider: provider("opencodezen", &config, None),
                    client: &denied,
                },
                &started
            )
            .await
            .unwrap(),
        DevicePoll::Denied
    ));

    let slow = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "slow_down"}).to_string(),
    );
    assert!(matches!(
        OpenCode::ZEN
            .poll(
                LoginContext {
                    provider: provider("opencodezen", &config, None),
                    client: &slow,
                },
                &started
            )
            .await
            .unwrap(),
        DevicePoll::SlowDown { interval_secs: 10 }
    ));

    let granted = Script::new(vec![
        (
            StatusCode::OK,
            json!({"access_token": "at", "refresh_token": "rt",
                   "token_type": "Bearer", "expires_in": 3600}),
        ),
        (
            StatusCode::OK,
            json!({"id": "usr_1", "email": "dev@example.com"}),
        ),
        (
            StatusCode::OK,
            json!([{"id": "org_b", "name": "Zeta"}, {"id": "org_a", "name": "Acme"}]),
        ),
    ]);
    let DevicePoll::Ready(acquired) = OpenCode::ZEN
        .poll(
            LoginContext {
                provider: provider("opencodezen", &config, None),
                client: &granted,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    let sent = granted.sent();
    assert_eq!(sent[0].0, "https://opencode.ai/console/auth/device/token");
    let mut lookups = vec![sent[1].0.as_str(), sent[2].0.as_str()];
    lookups.sort_unstable();
    assert_eq!(
        lookups,
        [
            "https://opencode.ai/console/api/orgs",
            "https://opencode.ai/console/api/user"
        ]
    );
    assert_eq!(sent[1].1["authorization"], "Bearer at");
    assert_eq!(acquired.access_token, "at");
    assert_eq!(acquired.refresh_token.as_deref(), Some("rt"));
    assert!(acquired.expires_at_ms.unwrap() > 0);
    let fields = &acquired.provider_fields;
    assert_eq!(
        fields["console_base_url"], "https://opencode.ai/console",
        "the refresh has to return to the console the login used"
    );
    assert_eq!(fields["account_id"], "usr_1");
    assert_eq!(fields["email"], "dev@example.com");
    assert_eq!(fields["org_id"], "org_a", "the first workspace by name");
    assert_eq!(fields["org_name"], "Acme");

    // A granted token outlives a failed account lookup: the device code is
    // already spent.
    let lookups_fail = Script::new(vec![
        (
            StatusCode::OK,
            json!({"access_token": "at", "refresh_token": "rt", "expires_in": 3600}),
        ),
        (StatusCode::INTERNAL_SERVER_ERROR, json!({})),
        (StatusCode::INTERNAL_SERVER_ERROR, json!({})),
    ]);
    let DevicePoll::Ready(bare) = OpenCode::ZEN
        .poll(
            LoginContext {
                provider: provider("opencodezen", &config, None),
                client: &lookups_fail,
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    assert_eq!(bare.access_token, "at");
    assert_eq!(
        bare.provider_fields.keys().collect::<Vec<_>>(),
        ["console_base_url"]
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
        provider: provider("opencodezen", &config, None),
        credential: credential("oauth", &secret, &Value::Null),
        client,
    };

    let rejected = OneShot::new(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant"}).to_string(),
    );
    assert!(matches!(
        CredentialRefresh::refresh(&OpenCode::ZEN, context(&rejected)).await,
        Err(ChannelError::RefreshRejected(_))
    ));

    let transient = OneShot::new(StatusCode::BAD_GATEWAY, "upstream down".into());
    assert!(
        matches!(
            CredentialRefresh::refresh(&OpenCode::ZEN, context(&transient)).await,
            Err(ChannelError::UpstreamResponse { .. })
        ),
        "a gateway failure is not a dead credential"
    );

    let rotated = OneShot::new(
        StatusCode::OK,
        json!({"access_token": "at2", "refresh_token": "rt2", "expires_in": 60}).to_string(),
    );
    let update = CredentialRefresh::refresh(&OpenCode::ZEN, context(&rotated))
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
    let update = CredentialRefresh::refresh(&OpenCode::ZEN, context(&silent))
        .await
        .unwrap();
    assert_eq!(update.expires_at_ms, None);
    assert!(update.secret.get("expires_at_ms").is_none());
}

#[test]
fn only_zen_offers_the_account_login() {
    let zen = OpenCode::ZEN.descriptor();
    assert_eq!(zen.id, "opencodezen");
    assert_eq!(zen.login_modes, [LoginMode::ApiKey, LoginMode::DeviceCode]);
    assert!(zen.capabilities.refresh);
    assert!(OpenCode::ZEN.oauth_device_code().is_some());
    assert!(OpenCode::ZEN.credential_refresh().is_some());
    for key in ["base_url", "console_base_url", "enable_openai_magic_cache"] {
        assert!(zen.config_key(key).is_some(), "{key}");
    }

    // The v2 client connects Go with a pasted key only.
    let go = OpenCode::GO.descriptor();
    assert_eq!(go.id, "opencodego");
    assert_eq!(go.login_modes, [LoginMode::ApiKey]);
    assert!(!go.capabilities.refresh);
    assert!(go.capabilities.quota_query);
    assert!(OpenCode::GO.oauth_device_code().is_none());
    assert!(OpenCode::GO.credential_refresh().is_none());
    assert!(go.config_key("console_base_url").is_none());
    assert!(go.config_key("tier").is_none() && zen.config_key("tier").is_none());

    for channel in [OpenCode::ZEN, OpenCode::GO] {
        assert!(
            channel.default_connection().is_none(),
            "v3 captured no client identity for OpenCode"
        );
        assert_eq!(
            channel.native_dialects(
                provider(channel.id(), &json!({}), None),
                Operation::GenerateContent
            ),
            [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
        );
    }
}
