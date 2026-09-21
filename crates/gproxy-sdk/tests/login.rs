//! Credential logins: what the SDK owns, and what it refuses.
//!
//! The contract under test is not "a token comes back". It is that the
//! verifier never travels, that the state is compared and a mismatch is final,
//! that a pending login lives in the cache and nowhere else, that one login is
//! exactly one revision, and that what lands in the database is sealed.

mod support;

use std::{collections::BTreeMap, sync::Arc};

use base64::Engine as _;
use gproxy_channel::channel::{AcquiredCredential, AuthorizationStart, DevicePoll};
use gproxy_sdk::{
    ClientPool, Gproxy, GproxyBuilder, SdkError, SyncMode,
    dto::{
        AuthCodeComplete, AuthCodeStart, CookieExchange, CredentialOwner, DevicePollOutcome,
        DeviceStart, ProviderWrite,
    },
};
use gproxy_store::entity::identity::{organization, user};
use sea_orm::{DatabaseConnection, Set};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

type Handle = Gproxy<DatabaseConnection>;

/// A handle whose credentials are really sealed, so a test can assert that the
/// plaintext is not in the column. The shared harness uses plaintext secrets,
/// which would make that assertion vacuous.
async fn sealed_sdk() -> (Handle, Arc<support::TestChannel>) {
    let channel = Arc::new(support::TestChannel::default());
    let gproxy = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .master_key([7u8; 32])
        .without_default_channels()
        .channel(channel.clone())
        .client_pool(ClientPool::with_client(Arc::new(
            support::ScriptClient::default(),
        )))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    (gproxy, channel)
}

async fn provider(gproxy: &Handle, channel: &str) -> String {
    gproxy
        .manage()
        .providers()
        .create(ProviderWrite {
            name: format!("{channel}-upstream"),
            channel: channel.to_owned(),
            base_url: Some("https://upstream.example".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
}

/// The durable revision, which is what a login has to advance by exactly one.
async fn durable(gproxy: &Handle) -> i64 {
    gproxy
        .store()
        .settings()
        .get()
        .await
        .unwrap()
        .unwrap()
        .config_revision
}

/// The parked session as it actually sits in the cache, which is the only
/// place it exists.
async fn session(gproxy: &Handle, id: &str) -> Option<Value> {
    let entry = gproxy
        .cache()
        .get(&format!("gproxy-sdk:v1:login:{id}"))
        .await
        .unwrap()?;
    Some(serde_json::from_slice(&entry.value).unwrap())
}

fn owner() -> CredentialOwner {
    CredentialOwner {
        organization_id: Some("org-1".into()),
        team_id: None,
        user_id: Some("u-1".into()),
    }
}

/// The rows [`owner`] names. The owner columns are real foreign keys, so a
/// login that names an owner needs one to exist — which is the application
/// layer's business, not this crate's.
async fn seed_owner(gproxy: &Handle) {
    gproxy
        .store()
        .organizations()
        .create_many(vec![organization::ActiveModel {
            id: Set("org-1".into()),
            name: Set("org one".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    gproxy
        .store()
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("u-1".into()),
            name: Set("person".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

fn s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[tokio::test]
async fn authcode_start_parks_the_verifier_and_publishes_only_its_challenge() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let before = durable(&gproxy).await;

    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id: provider_id.clone(),
            redirect_uri: Some("http://127.0.0.1:1455/callback".into()),
            label: None,
            owner: owner(),
        })
        .await
        .unwrap();

    // Starting a login writes nothing durable: there is no credential yet.
    assert_eq!(durable(&gproxy).await, before);

    let session = session(&gproxy, &started.login_session_id)
        .await
        .expect("the session is in the cache");
    assert_eq!(session["flow"], "auth_code");
    assert_eq!(session["provider_id"], provider_id.as_str());
    assert_eq!(session["channel"], "test");
    assert_eq!(session["redirect_uri"], "http://127.0.0.1:1455/callback");
    assert_eq!(session["owner"]["organizationId"], "org-1");

    let verifier = session["verifier"].as_str().unwrap();
    let state = session["state"].as_str().unwrap();
    // The URL carries the digest and the state, never the verifier itself.
    assert!(
        started.authorize_url.contains(&s256(verifier)),
        "the authorize URL must carry the S256 challenge: {}",
        started.authorize_url
    );
    assert!(!started.authorize_url.contains(verifier));
    assert!(started.authorize_url.contains(state));
    assert_eq!(started.redirect_uri, "http://127.0.0.1:1455/callback");

    // And that is exactly what the channel was handed.
    let calls = channel.login_calls.lines();
    assert_eq!(
        calls,
        vec![format!(
            "authorize {provider_id} state={state} challenge={}",
            s256(verifier)
        )]
    );
}

#[tokio::test]
async fn a_callback_url_completes_the_login_into_a_sealed_credential() {
    let (gproxy, channel) = sealed_sdk().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id: provider_id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    let parked = session(&gproxy, &started.login_session_id).await.unwrap();
    let state = parked["state"].as_str().unwrap().to_owned();
    let verifier = parked["verifier"].as_str().unwrap().to_owned();
    channel
        .code_exchanges
        .lock()
        .unwrap()
        .push_back(Ok(support::oauth_credential("access-abcdef")));

    let before = durable(&gproxy).await;
    let created = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id.clone(),
            callback_url: Some(format!(
                "http://127.0.0.1/callback?code=auth-code&state={state}"
            )),
            ..Default::default()
        })
        .await
        .unwrap();

    // One login, one revision.
    assert_eq!(durable(&gproxy).await, before + 1);
    // The session is spent.
    assert!(session(&gproxy, &started.login_session_id).await.is_none());

    // The verifier reached the upstream only now, at the exchange.
    assert_eq!(
        channel.login_calls.lines().last().unwrap(),
        &format!("exchange code=auth-code verifier={verifier} state={state}")
    );

    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&created.credential_id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(row.provider_id, provider_id);
    assert_eq!(row.auth_kind, "oauth");
    assert_eq!(row.version, 0);
    assert!(row.enabled);
    assert!(
        !row.secret
            .windows(b"access-abcdef".len())
            .any(|window| window == b"access-abcdef"),
        "the sealed column must not contain the access token"
    );
    assert_eq!(
        gproxy
            .manage()
            .credentials()
            .reveal_secret(&created.credential_id)
            .await
            .unwrap()["access_token"],
        "access-abcdef"
    );
    // A new row is not in the snapshot until a full reload, which the commit
    // performed: the credential is usable immediately.
    assert!(
        gproxy
            .core()
            .snapshot()
            .credentials
            .contains_key(&created.credential_id)
    );
}

#[tokio::test]
async fn a_bare_code_completes_the_login_too() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            label: Some("  pasted by hand  ".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let state = session(&gproxy, &started.login_session_id).await.unwrap()["state"]
        .as_str()
        .unwrap()
        .to_owned();
    channel
        .code_exchanges
        .lock()
        .unwrap()
        .push_back(Ok(support::oauth_credential("access-2")));

    let created = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id,
            code: Some("bare-code".into()),
            state: Some(state),
            ..Default::default()
        })
        .await
        .unwrap();

    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&created.credential_id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    // A caller-supplied label wins, trimmed.
    assert_eq!(row.label.as_deref(), Some("pasted by hand"));
}

/// The authorization-code pocket: a channel's own facts survive the person's
/// trip through the browser, in the session and nowhere else.
#[tokio::test]
async fn an_authorization_code_login_round_trips_the_channels_provider_state() {
    let (gproxy, channel) = sealed_sdk().await;
    let provider_id = provider(&gproxy, "test").await;
    // A client the channel registered for this login alone: an id that is a
    // public fact and a secret that is not.
    channel
        .authorize_starts
        .lock()
        .unwrap()
        .push_back(AuthorizationStart {
            authorize_url: "https://login.example/authorize".into(),
            redirect_uri: "http://127.0.0.1:1455/callback".into(),
            provider_state: BTreeMap::from([
                ("client_id".into(), json!("registered-9")),
                ("client_secret".into(), json!("registered-secret")),
            ]),
        });

    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();
    let parked = session(&gproxy, &started.login_session_id).await.unwrap();
    assert_eq!(parked["provider_state"]["client_id"], "registered-9");
    assert_eq!(
        parked["provider_state"]["client_secret"], "registered-secret",
        "the cache-backed session is where a login's own secret waits"
    );

    channel
        .code_exchanges
        .lock()
        .unwrap()
        .push_back(Ok(support::oauth_credential("access-3")));
    let state = parked["state"].as_str().unwrap().to_owned();
    let created = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id,
            code: Some("code-3".into()),
            state: Some(state),
            ..Default::default()
        })
        .await
        .unwrap();

    // What was parked is what the exchange was handed, unchanged.
    let handed = channel.exchanged_provider_state.lock().unwrap().clone();
    assert_eq!(
        handed,
        vec![BTreeMap::from([
            ("client_id".to_owned(), json!("registered-9")),
            ("client_secret".to_owned(), json!("registered-secret")),
        ])]
    );

    // And none of it reached the credential row, whose metadata is rendered.
    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&created.credential_id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert!(
        !row.metadata.to_string().contains("registered"),
        "provider_state must not become credential metadata: {}",
        row.metadata
    );
}

#[tokio::test]
async fn a_login_that_carries_no_provider_state_carries_an_empty_one() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();
    let parked = session(&gproxy, &started.login_session_id).await.unwrap();
    assert_eq!(parked["provider_state"], json!({}));

    channel
        .code_exchanges
        .lock()
        .unwrap()
        .push_back(Ok(support::oauth_credential("access-4")));
    let state = parked["state"].as_str().unwrap().to_owned();
    gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id,
            code: Some("code-4".into()),
            state: Some(state),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        channel.exchanged_provider_state.lock().unwrap().clone(),
        vec![BTreeMap::new()],
        "empty on one side is empty on the other, never absent"
    );
}

#[tokio::test]
async fn a_callback_url_and_a_code_together_are_refused() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();

    let error = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id.clone(),
            callback_url: Some("http://127.0.0.1/callback?code=a&state=b".into()),
            code: Some("c".into()),
            state: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::Invalid(_)), "{error}");
    // An ambiguous request is a caller mistake, not an attack: the session
    // survives so the caller can send the right one.
    assert!(session(&gproxy, &started.login_session_id).await.is_some());

    // Neither is refused for the same reason.
    assert!(matches!(
        gproxy
            .login()
            .authcode_complete(AuthCodeComplete {
                login_session_id: started.login_session_id,
                ..Default::default()
            })
            .await
            .unwrap_err(),
        SdkError::Invalid(_)
    ));
}

#[tokio::test]
async fn a_state_that_does_not_match_destroys_the_session() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();

    let error = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id.clone(),
            callback_url: Some("http://127.0.0.1/callback?code=a&state=forged".into()),
            ..Default::default()
        })
        .await
        .unwrap_err();
    match &error {
        SdkError::Invalid(message) => assert_eq!(message, "authorization state mismatch"),
        other => panic!("expected a state mismatch, got {other}"),
    }
    // Nothing was exchanged, and the session cannot be retried into success.
    assert!(
        channel
            .login_calls
            .lines()
            .iter()
            .all(|line| !line.starts_with("exchange"))
    );
    assert!(session(&gproxy, &started.login_session_id).await.is_none());
    assert!(matches!(
        gproxy
            .login()
            .authcode_complete(AuthCodeComplete {
                login_session_id: started.login_session_id,
                code: Some("a".into()),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        SdkError::LoginExpired
    ));
}

#[tokio::test]
async fn an_expired_or_unknown_session_is_gone_rather_than_missing() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;

    assert!(matches!(
        gproxy
            .login()
            .authcode_complete(AuthCodeComplete {
                login_session_id: "never-existed".into(),
                code: Some("a".into()),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        SdkError::LoginExpired
    ));
    assert!(matches!(
        gproxy
            .login()
            .device_poll("never-existed")
            .await
            .unwrap_err(),
        SdkError::LoginExpired
    ));

    // An expired key and a key that was never written are the same thing to
    // the cache, so dropping the key is exactly how expiry presents.
    let started = gproxy
        .login()
        .authcode_start(AuthCodeStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();
    gproxy
        .cache()
        .delete(&format!("gproxy-sdk:v1:login:{}", started.login_session_id))
        .await
        .unwrap();
    let error = gproxy
        .login()
        .authcode_complete(AuthCodeComplete {
            login_session_id: started.login_session_id,
            code: Some("a".into()),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(matches!(error, SdkError::LoginExpired));
    assert_eq!(error.status_code(), 410);
}

#[tokio::test]
async fn a_device_login_polls_until_it_is_ready() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    seed_owner(&gproxy).await;
    let mut authorization = support::device_authorization("dev-9", "WXYZ-1234");
    authorization.verification_uri_complete = Some("https://login.example/device?c=WXYZ".into());
    // Provider-specific state must survive the round trip through the cache;
    // a device flow that is not RFC 8628 keeps its handle here.
    authorization
        .provider_state
        .insert("device_auth_id".into(), json!("auth-77"));
    channel
        .device_authorizations
        .lock()
        .unwrap()
        .push_back(authorization);

    let started = gproxy
        .login()
        .device_start(DeviceStart {
            provider_id: provider_id.clone(),
            label: None,
            owner: owner(),
        })
        .await
        .unwrap();
    assert_eq!(started.user_code, "WXYZ-1234");
    assert_eq!(
        started.verification_uri_complete.as_deref(),
        Some("https://login.example/device?c=WXYZ")
    );
    assert_eq!(started.interval_secs, 5);

    let parked = session(&gproxy, &started.login_session_id).await.unwrap();
    assert_eq!(parked["flow"], "device");
    assert_eq!(parked["interval_secs"], 5);
    assert_eq!(
        parked["authorization"]["provider_state"]["device_auth_id"],
        "auth-77"
    );

    {
        let mut polls = channel.device_polls.lock().unwrap();
        polls.push_back(DevicePoll::Pending);
        polls.push_back(DevicePoll::SlowDown { interval_secs: 30 });
        polls.push_back(DevicePoll::Ready(support::oauth_credential("device-token")));
    }

    let login = gproxy.login();
    assert!(matches!(
        login.device_poll(&started.login_session_id).await.unwrap(),
        DevicePollOutcome::Pending { interval_secs: 5 }
    ));
    // A slow-down is still "wait, then ask again", with a new cadence.
    assert!(matches!(
        login.device_poll(&started.login_session_id).await.unwrap(),
        DevicePollOutcome::Pending { interval_secs: 30 }
    ));
    let parked = session(&gproxy, &started.login_session_id).await.unwrap();
    assert_eq!(parked["interval_secs"], 30);
    // The raised interval is what a later Pending would report, and the
    // authorization it was stored beside is unchanged.
    assert_eq!(
        parked["authorization"]["provider_state"]["device_auth_id"],
        "auth-77"
    );

    let before = durable(&gproxy).await;
    let DevicePollOutcome::Ready { credential_id } =
        login.device_poll(&started.login_session_id).await.unwrap()
    else {
        panic!("the third poll is ready");
    };
    assert_eq!(durable(&gproxy).await, before + 1);
    assert!(session(&gproxy, &started.login_session_id).await.is_none());

    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&credential_id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(row.auth_kind, "oauth");
    assert_eq!(row.organization_id.as_deref(), Some("org-1"));
    assert_eq!(row.user_id.as_deref(), Some("u-1"));
    assert_eq!(row.team_id, None);
    // Every poll reached the channel with the authorization it was given.
    assert_eq!(
        channel
            .login_calls
            .lines()
            .iter()
            .filter(|line| line.as_str() == "device-poll dev-9")
            .count(),
        3
    );
}

#[tokio::test]
async fn a_denied_device_login_is_over() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .device_start(DeviceStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();
    channel
        .device_polls
        .lock()
        .unwrap()
        .push_back(DevicePoll::Denied);

    let before = durable(&gproxy).await;
    assert!(matches!(
        gproxy
            .login()
            .device_poll(&started.login_session_id)
            .await
            .unwrap(),
        DevicePollOutcome::Denied
    ));
    assert_eq!(durable(&gproxy).await, before);
    assert!(session(&gproxy, &started.login_session_id).await.is_none());
    assert!(matches!(
        gproxy
            .login()
            .device_poll(&started.login_session_id)
            .await
            .unwrap_err(),
        SdkError::LoginExpired
    ));
}

#[tokio::test]
async fn an_expired_device_login_is_over_too() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let started = gproxy
        .login()
        .device_start(DeviceStart {
            provider_id,
            ..Default::default()
        })
        .await
        .unwrap();
    channel
        .device_polls
        .lock()
        .unwrap()
        .push_back(DevicePoll::Expired);

    assert!(matches!(
        gproxy
            .login()
            .device_poll(&started.login_session_id)
            .await
            .unwrap(),
        DevicePollOutcome::Expired
    ));
    assert!(session(&gproxy, &started.login_session_id).await.is_none());
}

#[tokio::test]
async fn a_cookie_exchange_creates_a_cookie_credential() {
    let (gproxy, channel) = sealed_sdk().await;
    let provider_id = provider(&gproxy, "test").await;
    channel
        .cookie_exchanges
        .lock()
        .unwrap()
        .push_back(Ok(AcquiredCredential {
            secret: json!({ "cookie": "sessionKey=sk-ant-0123456789" }),
            expires_at_ms: Some(1_800_000_000_000),
            metadata: json!({ "user_email": "person@example.com", "rate_limit_tier": "max" }),
        }));

    let before = durable(&gproxy).await;
    let created = gproxy
        .login()
        .cookie_exchange(CookieExchange {
            provider_id: provider_id.clone(),
            cookie: "sessionKey=sk-ant-0123456789".into(),
            label: None,
            owner: CredentialOwner::default(),
        })
        .await
        .unwrap();
    assert_eq!(durable(&gproxy).await, before + 1);

    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&created.credential_id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(row.auth_kind, "cookie");
    assert_eq!(row.expires_at_ms, Some(1_800_000_000_000));
    // The metadata is public and stays readable; the cookie itself does not.
    assert_eq!(row.metadata["user_email"], "person@example.com");
    assert!(
        !row.secret
            .windows(b"sk-ant-0123456789".len())
            .any(|window| window == b"sk-ant-0123456789")
    );
    // The default label names the channel and the account it identified.
    assert_eq!(
        row.label.as_deref(),
        Some("test person@example.com max"),
        "the default label carries the channel and the account"
    );

    // A blank cookie never reaches the channel.
    assert!(matches!(
        gproxy
            .login()
            .cookie_exchange(CookieExchange {
                provider_id,
                cookie: "   ".into(),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        SdkError::Invalid(_)
    ));
}

#[tokio::test]
async fn default_labels_do_not_collide_on_one_provider() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider_id = provider(&gproxy, "test").await;
    let mut labels = Vec::new();
    for index in 0..3 {
        channel
            .cookie_exchanges
            .lock()
            .unwrap()
            .push_back(Ok(AcquiredCredential {
                secret: json!({ "cookie": format!("cookie-{index}") }),
                expires_at_ms: None,
                metadata: Value::Null,
            }));
        let created = gproxy
            .login()
            .cookie_exchange(CookieExchange {
                provider_id: provider_id.clone(),
                cookie: format!("cookie-{index}"),
                ..Default::default()
            })
            .await
            .unwrap();
        let row = gproxy
            .store()
            .credentials()
            .get_many(std::slice::from_ref(&created.credential_id))
            .await
            .unwrap()
            .remove(0)
            .unwrap();
        // Metadata that says nothing still becomes an object, never null.
        assert_eq!(row.metadata, json!({}));
        labels.push(row.label.unwrap());
    }
    assert_eq!(labels, vec!["test", "test (2)", "test (3)"]);
}

#[tokio::test]
async fn a_channel_without_the_flow_says_so() {
    // `AltChannel` implements none of the three login traits.
    let (gproxy, _, _) = support::seed::handle().await;
    support::seed::provider(&gproxy, "p-alt", "alt", &[]).await;
    support::seed::publish(&gproxy).await;
    let login = gproxy.login();

    for error in [
        login
            .authcode_start(AuthCodeStart {
                provider_id: "p-alt".into(),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        login
            .device_start(DeviceStart {
                provider_id: "p-alt".into(),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        login
            .cookie_exchange(CookieExchange {
                provider_id: "p-alt".into(),
                cookie: "whatever".into(),
                ..Default::default()
            })
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, SdkError::Unsupported(_)), "{error}");
        assert_eq!(error.status_code(), 501);
    }

    // A provider nobody created is a 404, not an unsupported flow.
    assert!(matches!(
        login
            .device_start(DeviceStart {
                provider_id: "absent".into(),
                ..Default::default()
            })
            .await
            .unwrap_err(),
        SdkError::NotFound { .. }
    ));
}
