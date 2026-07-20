use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use bytes::Bytes;
use http::StatusCode;
use serde_json::json;

use crate::channel::{Disposition, PrepareCtx, PreparedRequest, TransportKind};
use crate::crypto::envelope::is_envelope;
use crate::http::client::{ClientError, UpstreamClient};
use crate::store::cache::{CacheBackend, MemoryCache};
use crate::store::persistence::DbPersistence;
use crate::store::persistence::records::{CredentialInput, Provider};

/// Minimal provider record for refresh tests (no proxy / TLS override, so
/// the resolved refresh client is the default pooled client).
fn test_provider() -> Provider {
    Provider {
        id: 1,
        name: "p".into(),
        channel: "fake_refresh".into(),
        label: None,
        settings_json: json!({}),
        credential_strategy: "round_robin".into(),
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

/// Channel whose refresh emits `{"access_token":"new"}` and is "stale" until
/// the secret carries that marker — so a loser's re-check short-circuits.
struct FakeRefreshChannel {
    refreshes: Arc<AtomicUsize>,
    sleep_ms: u64,
}

#[async_trait]
impl Channel for FakeRefreshChannel {
    fn id(&self) -> &'static str {
        "fake_refresh"
    }
    fn provider_family(&self) -> crate::protocol::Provider {
        crate::protocol::Provider::OpenAi
    }
    fn routing_table(&self) -> crate::channel::routes::RouteList {
        Vec::new()
    }
    fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Unsupported("prepare"))
    }
    fn classify(
        &self,
        status: StatusCode,
        headers: &http::HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        Disposition::from_http(status, headers)
    }
    fn transport(&self) -> TransportKind {
        TransportKind::Http
    }
    fn needs_refresh(&self, secret: &Value) -> bool {
        secret.get("access_token").and_then(Value::as_str) != Some("new")
    }
    async fn refresh(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _secret: &Value,
    ) -> Result<Value, ChannelError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        if self.sleep_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.sleep_ms)).await;
        }
        Ok(json!({"access_token": "new"}))
    }
}

struct NoopUpstream;
#[async_trait]
impl UpstreamClient for NoopUpstream {
    async fn send(&self, _req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        Err(ClientError::Transport("noop".into()))
    }
}

fn cipher() -> Arc<dyn crate::crypto::SecretCipher> {
    crate::crypto::cipher_from_master_key(Some(&B64.encode([9u8; 32]))).unwrap()
}

struct TestState {
    persistence: Arc<dyn PersistenceBackend>,
    cache: Arc<dyn CacheBackend>,
    cipher: Arc<dyn SecretCipher>,
    upstream: Arc<dyn UpstreamClient>,
    refresh: RefreshOrchestrator,
}

impl TestState {
    async fn ensure_fresh_credential(
        &self,
        channel: &Arc<dyn Channel>,
        credential: &Credential,
        _provider: &Provider,
        opened: Value,
        force: bool,
    ) -> Result<Value, ChannelError> {
        let resolve_client = || Ok(Arc::clone(&self.upstream));
        self.refresh
            .ensure_fresh(
                RefreshDeps {
                    persistence: self.persistence.as_ref(),
                    cache: self.cache.as_ref(),
                    cipher: self.cipher.as_ref(),
                    resolve_client: &resolve_client,
                },
                channel,
                credential,
                opened,
                force,
            )
            .await
    }
}

/// Minimal refresh dependencies over in-memory persistence, seeded with one
/// credential whose secret is `seed` (sealed).
async fn state_with_cred(
    cipher: Arc<dyn crate::crypto::SecretCipher>,
    seed: Value,
) -> (TestState, Credential, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
        DbPersistence::connect("sqlite::memory:")
            .await
            .expect("db persistence"),
    );
    let sealed = cipher.seal(&seed).unwrap();
    let credential = PersistenceBackend::upsert_credential(
        persistence.as_ref(),
        CredentialInput {
            id: None,
            provider_id: 1,
            name: Some("c".into()),
            kind: "oauth".into(),
            secret_json: sealed,
            weight: 100,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
            enabled: true,
        },
    )
    .await
    .expect("seed credential");
    let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCache::new());
    let upstream: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);
    let state = TestState {
        cache,
        persistence,
        upstream,
        cipher,
        refresh: RefreshOrchestrator::new(),
    };
    (state, credential, dir)
}

/// Read the sealed secret currently stored for `cred`.
async fn stored_secret(state: &TestState, cred: &Credential) -> Value {
    PersistenceBackend::list_credentials(state.persistence.as_ref(), cred.provider_id)
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == cred.id)
        .unwrap()
        .secret_json
}

#[tokio::test]
async fn refreshes_and_writes_back_sealed() {
    let cipher = cipher();
    let (state, cred, _dir) = state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 0,
    });

    let got = state
        .ensure_fresh_credential(
            &channel,
            &cred,
            &test_provider(),
            json!({"access_token": "old"}),
            false,
        )
        .await
        .unwrap();

    assert_eq!(got, json!({"access_token": "new"}));
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    // Persisted secret is a real envelope that opens to the refreshed value.
    let stored = stored_secret(&state, &cred).await;
    assert!(is_envelope(&stored), "stored secret should be sealed");
    assert_eq!(
        cipher.open(&stored).unwrap(),
        json!({"access_token": "new"})
    );
}

#[tokio::test]
async fn refresh_does_not_recreate_deleted_credential() {
    let cipher = cipher();
    let (state, cred, _dir) = state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 50,
    });
    let provider = test_provider();

    let (result, _) = tokio::join!(
        state.ensure_fresh_credential(
            &channel,
            &cred,
            &provider,
            json!({"access_token": "old"}),
            false,
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            PersistenceBackend::delete_credential(state.persistence.as_ref(), cred.id)
                .await
                .expect("delete credential");
        },
    );

    assert!(result.is_err(), "refresh must not use a deleted credential");
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert!(
        PersistenceBackend::get_credential(state.persistence.as_ref(), cred.id)
            .await
            .unwrap()
            .is_none(),
        "refresh writeback must not reinsert the credential"
    );
}

#[tokio::test]
async fn refresh_does_not_reenable_disabled_credential() {
    let cipher = cipher();
    let (state, cred, _dir) = state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 50,
    });
    let provider = test_provider();
    let original_secret = cred.secret_json.clone();

    let (result, _) = tokio::join!(
        state.ensure_fresh_credential(
            &channel,
            &cred,
            &provider,
            json!({"access_token": "old"}),
            false,
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            PersistenceBackend::upsert_credential(
                state.persistence.as_ref(),
                CredentialInput {
                    id: Some(cred.id),
                    provider_id: cred.provider_id,
                    name: cred.name.clone(),
                    kind: cred.kind.clone(),
                    secret_json: original_secret,
                    weight: cred.weight,
                    rpm_limit: cred.rpm_limit,
                    tpm_limit: cred.tpm_limit,
                    proxy_url: cred.proxy_url.clone(),
                    tls_fingerprint: cred.tls_fingerprint.clone(),
                    enabled: false,
                },
            )
            .await
            .expect("disable credential");
        },
    );

    assert!(
        result.is_err(),
        "refresh must not use a disabled credential"
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    let stored = PersistenceBackend::get_credential(state.persistence.as_ref(), cred.id)
        .await
        .unwrap()
        .expect("credential remains disabled");
    assert!(!stored.enabled, "refresh writeback must not re-enable");
    assert_eq!(
        cipher.open(&stored.secret_json).unwrap(),
        json!({"access_token": "old"})
    );
}

#[tokio::test]
async fn no_refresh_when_fresh() {
    let cipher = cipher();
    let fresh = json!({"access_token": "new"});
    let (state, cred, _dir) = state_with_cred(cipher.clone(), fresh.clone()).await;
    let before = stored_secret(&state, &cred).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 0,
    });

    let got = state
        .ensure_fresh_credential(&channel, &cred, &test_provider(), fresh.clone(), false)
        .await
        .unwrap();

    assert_eq!(got, fresh);
    assert_eq!(refreshes.load(Ordering::SeqCst), 0, "refresh must not run");
    // Persistence untouched.
    assert_eq!(stored_secret(&state, &cred).await, before);
}

#[tokio::test]
async fn single_flight_refreshes_once() {
    let cipher = cipher();
    let (state, cred, _dir) = state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 20,
    });

    let stale = json!({"access_token": "old"});
    let provider = test_provider();
    let (a, b) = tokio::join!(
        state.ensure_fresh_credential(&channel, &cred, &provider, stale.clone(), false),
        state.ensure_fresh_credential(&channel, &cred, &provider, stale.clone(), false),
    );

    assert_eq!(a.unwrap(), json!({"access_token": "new"}));
    assert_eq!(b.unwrap(), json!({"access_token": "new"}));
    // Loser re-reads the winner's sealed result and short-circuits.
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

/// Channel that ALWAYS reports fresh (`needs_refresh == false`) yet whose
/// `refresh` rotates the token — models the forced-refresh case where the
/// staleness view can't distinguish winner from loser, so the loser must
/// fall back on "the secret changed under the lock".
struct AlwaysFreshRotatingChannel {
    refreshes: Arc<AtomicUsize>,
    sleep_ms: u64,
}

#[async_trait]
impl Channel for AlwaysFreshRotatingChannel {
    fn id(&self) -> &'static str {
        "always_fresh"
    }
    fn provider_family(&self) -> crate::protocol::Provider {
        crate::protocol::Provider::OpenAi
    }
    fn routing_table(&self) -> crate::channel::routes::RouteList {
        Vec::new()
    }
    fn prepare(&self, _ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Unsupported("prepare"))
    }
    fn classify(
        &self,
        status: StatusCode,
        headers: &http::HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        Disposition::from_http(status, headers)
    }
    fn transport(&self) -> TransportKind {
        TransportKind::Http
    }
    fn needs_refresh(&self, _secret: &Value) -> bool {
        false
    }
    async fn refresh(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _secret: &Value,
    ) -> Result<Value, ChannelError> {
        let n = self.refreshes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.sleep_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.sleep_ms)).await;
        }
        Ok(json!({ "access_token": format!("rotated-{n}") }))
    }
}

/// Two concurrent FORCED refreshes (AuthDead on both) of the same credential
/// must rotate the token exactly once. A single-use refresh_token rotated
/// twice would be killed upstream; the loser sees the secret changed under
/// the lock and reuses the winner's token instead of refreshing again.
#[tokio::test]
async fn forced_single_flight_rotates_once() {
    let cipher = cipher();
    let (state, cred, _dir) =
        state_with_cred(cipher.clone(), json!({"access_token": "orig"})).await;
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(AlwaysFreshRotatingChannel {
        refreshes: refreshes.clone(),
        sleep_ms: 20,
    });

    let orig = json!({"access_token": "orig"});
    let provider = test_provider();
    let (a, b) = tokio::join!(
        state.ensure_fresh_credential(&channel, &cred, &provider, orig.clone(), true),
        state.ensure_fresh_credential(&channel, &cred, &provider, orig.clone(), true),
    );

    // Exactly one rotation; both callers see the same rotated token.
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(a.unwrap(), b.unwrap());
}
