//! Single-flight OAuth credential refresh (§14.5). Lazy: only when the channel
//! says the decrypted secret needs it. Per-credential local mutex serialises
//! concurrent refreshes within an instance (many providers rotate refresh_token
//! each call, so a double refresh kills the credential). Across instances, a
//! best-effort redis lock (key = `gproxy:refresh:lock:{cred id}`) wraps the
//! upstream refresh call so two instances cannot rotate a single-use token at
//! once; the loser waits briefly, re-reads, and reuses the winner's result.
//! The redis lock is a no-op `true` on memory/edge backends (single instance).
//!
//! The mutex is `futures_util::lock::Mutex` (runtime-agnostic): tokio is a
//! native-only dependency, so the edge/wasm build cannot use `tokio::sync`.

use std::sync::Arc;

use dashmap::DashMap;
use futures_util::lock::Mutex;
use serde_json::Value;

use crate::app::AppState;
use crate::channel::{Channel, ChannelError};
use crate::store::persistence::records::{Credential, Provider};

/// Serialises refreshes per credential id so concurrent requests cannot rotate
/// the same credential twice.
pub struct RefreshOrchestrator {
    locks: DashMap<i64, Arc<Mutex<()>>>,
}

impl Default for RefreshOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl RefreshOrchestrator {
    pub fn new() -> Self {
        Self {
            locks: DashMap::new(),
        }
    }

    /// Return a fresh decrypted secret for the credential, refreshing if the
    /// channel deems it stale. `opened` is the already-decrypted current secret.
    /// `force` skips the staleness gate (AuthDead-triggered forced refresh).
    pub async fn ensure_fresh(
        &self,
        state: &AppState,
        channel: &Arc<dyn Channel>,
        credential: &Credential,
        provider: &Provider,
        opened: Value,
        force: bool,
    ) -> Result<Value, ChannelError> {
        if !force && !channel.needs_refresh(&opened) {
            return Ok(opened);
        }
        let lock = self
            .locks
            .entry(credential.id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        // Loser re-check (single-flight): re-read the credential + re-open. Two
        // discriminators, because `force` and the lazy path differ:
        //   * lazy (force=false): a peer that rotated leaves the secret no longer
        //     stale — `needs_refresh(&current)` is false → use it, no 2nd refresh.
        //   * forced (force=true): the token may still LOOK fresh (the AuthDead is
        //     clock-skew / server-side revocation), so `needs_refresh` can't tell
        //     winner from loser. Instead compare against what THIS caller opened:
        //     if the re-read secret CHANGED, a concurrent forced refresh already
        //     rotated it → use that (a 2nd rotation would double-spend a single-use
        //     refresh_token and kill the cred). If unchanged, this caller is the
        //     winner and must honor `force`.
        let mut current = reread_open_enabled(state, credential)
            .await
            .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
            .ok_or_else(|| {
                ChannelError::Build("credential changed or disabled during refresh".into())
            })?;
        if !force && !channel.needs_refresh(&current.secret) {
            return Ok(current.secret);
        }
        if force && current.secret != opened {
            return Ok(current.secret);
        }
        // §7.4: resolve the effective (proxy, TLS fingerprint) for THIS
        // credential and refresh through the matching pooled client — mirroring
        // `failover::attempt`. A credential pinned to an egress proxy / TLS
        // profile then refreshes from the same identity it serves traffic from
        // (some providers risk-score token refreshes by source IP, and a
        // refresh from the host's bare IP can trip revocation + leaks that IP to
        // the token endpoint). Resolved BEFORE the redis lock so a bad-target
        // failure never leaks the lock; an unusable target fails the refresh
        // (cool + skip), never a silent downgrade to the default client. wasm /
        // non-wreq builds always use the default client.
        #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
        let client = {
            let global_proxy = state.upstream_proxy_url();
            let proxy = crate::channel::resolve::effective_proxy(
                credential,
                provider,
                global_proxy.as_deref(),
            );
            let fingerprint =
                crate::channel::resolve::effective_tls_fingerprint(credential, provider);
            let resolved = if let Some(fp) = fingerprint.as_ref() {
                state.client_pool.for_target(proxy.as_deref(), Some(fp))
            } else if let Some(emu) = channel.default_emulation() {
                state
                    .client_pool
                    .for_channel(proxy.as_deref(), channel.id(), emu)
            } else {
                state.client_pool.for_target(proxy.as_deref(), None)
            };
            resolved.map_err(|e| ChannelError::Build(format!("resolve refresh client: {e}")))?
        };
        #[cfg(not(all(not(target_arch = "wasm32"), feature = "upstream-wreq")))]
        let client = {
            let _ = provider; // proxy/fingerprint policy is native+wreq only
            Arc::clone(&state.upstream)
        };
        // Cross-instance single-flight: the local mutex above serialises this
        // instance, but a single-use refresh_token must not be rotated by two
        // instances at once. Acquire a best-effort redis lock around the actual
        // upstream refresh. Default-true on memory/edge, so single-instance and
        // wasm builds take the fast path (always `acquired`).
        let lock_key = format!("gproxy:refresh:lock:{}", credential.id);
        let acquired = state
            .cache
            .try_lock(&lock_key, std::time::Duration::from_secs(30))
            .await;
        if !acquired {
            // Another instance is rotating this credential. Wait briefly, re-read,
            // and reuse its result if it landed — avoids a second rotation. The
            // wait is native-only (tokio); on wasm `acquired` is always true via
            // the default, so this branch is unreachable there.
            #[cfg(not(target_arch = "wasm32"))]
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let peer = reread_open_enabled(state, credential)
                .await
                .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
                .ok_or_else(|| {
                    ChannelError::Build("credential changed or disabled during refresh".into())
                })?;
            if !force && !channel.needs_refresh(&peer.secret) {
                return Ok(peer.secret);
            }
            if force && peer.secret != opened {
                return Ok(peer.secret);
            }
            // Still stale after the wait — fall through and refresh anyway
            // (bounded: we tried once and the peer didn't land in time).
            current = peer;
        }
        // Bind the Result so the redis lock is released on EVERY exit path —
        // including the error path — before `?` propagates. Never hold the lock
        // across seal/writeback/publish; release right after the upstream call.
        let fresh = channel.refresh(&client, &current.secret).await;
        if acquired {
            state.cache.unlock(&lock_key).await;
        }
        let fresh = fresh?;
        // seal + writeback + publish — channel error already propagated above so
        // the caller cools + skips the credential on a failed refresh.
        let sealed = state
            .cipher
            .seal(&fresh)
            .map_err(|e| ChannelError::Build(format!("seal refreshed secret: {e}")))?;
        writeback(state, credential, current.updated_at, sealed)
            .await
            .map_err(|e| ChannelError::Build(format!("persist refreshed secret: {e}")))?;
        crate::app::invalidation::broadcast(
            state.cache.as_ref(),
            format!("cred:{}", credential.id).as_bytes(),
        )
        .await;
        Ok(fresh)
    }
}

struct OpenCredential {
    secret: Value,
    updated_at: i64,
}

/// Re-read the credential from persistence and decrypt its secret. Missing,
/// disabled, or provider-mismatched credentials mean an admin changed the
/// record while refresh was in flight; callers must stop using it.
async fn reread_open_enabled(
    state: &AppState,
    credential: &Credential,
) -> anyhow::Result<Option<OpenCredential>> {
    let Some(stored) = state.persistence.get_credential(credential.id).await? else {
        return Ok(None);
    };
    if stored.provider_id != credential.provider_id || !stored.enabled {
        return Ok(None);
    }
    Ok(Some(OpenCredential {
        secret: state.cipher.open(&stored.secret_json)?,
        updated_at: stored.updated_at,
    }))
}

/// Persist only the re-sealed secret. This is deliberately not a generic
/// upsert: it must not insert a deleted credential, re-enable a disabled one,
/// or overwrite any admin-edited fields from the stale snapshot.
async fn writeback(
    state: &AppState,
    credential: &Credential,
    expected_updated_at: i64,
    sealed: Value,
) -> anyhow::Result<()> {
    let updated = state
        .persistence
        .update_credential_secret_if_current(
            credential.id,
            credential.provider_id,
            expected_updated_at,
            sealed,
        )
        .await?;
    anyhow::ensure!(updated, "credential changed or disabled during refresh");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use arc_swap::ArcSwap;
    use async_trait::async_trait;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use bytes::Bytes;
    use http::StatusCode;
    use serde_json::json;

    use crate::app::snapshot::ControlPlaneSnapshot;
    use crate::channel::{Disposition, PrepareCtx, PreparedRequest, TransportKind};
    use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
    use crate::crypto::envelope::is_envelope;
    use crate::http::client::{ClientError, UpstreamClient};
    use crate::store::persistence::FilePersistence;
    use crate::store::persistence::records::CredentialInput;

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
        async fn send(
            &self,
            _req: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, ClientError> {
            Err(ClientError::Transport("noop".into()))
        }
    }

    fn cipher() -> Arc<dyn crate::crypto::SecretCipher> {
        crate::crypto::cipher_from_master_key(Some(&B64.encode([9u8; 32]))).unwrap()
    }

    /// AppState over a FilePersistence tempdir + MemoryCache + EnvelopeCipher,
    /// seeded with one credential whose secret is `seed` (sealed).
    async fn state_with_cred(
        cipher: Arc<dyn crate::crypto::SecretCipher>,
        seed: Value,
    ) -> (AppState, Credential, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
            FilePersistence::open(dir.path().to_path_buf())
                .await
                .expect("file persistence"),
        );
        let sealed = cipher.seal(&seed).unwrap();
        let credential = persistence
            .upsert_credential(CredentialInput {
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
            })
            .await
            .expect("seed credential");
        let config = Arc::new(RuntimeConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cache: CacheConfig::Memory,
            persistence: PersistenceConfig::File {
                data_dir: dir.path().to_path_buf(),
            },
            upstream: UpstreamConfig::from_proxy_url(None),
            instance_id: 0,
            max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
            max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
            trusted_proxies: Vec::new(),
            update_repo: None,
            update_channel: "releases".to_string(),
            update_data_dir: dir.path().to_path_buf(),
            cors_origins: Vec::new(),
        });
        let cache: Arc<dyn crate::store::cache::CacheBackend> =
            Arc::new(crate::store::cache::MemoryCache::new());
        let upstream: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);
        let snapshot = Arc::new(ArcSwap::from_pointee(ControlPlaneSnapshot::empty(1)));
        let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());
        let state = AppState::new(
            config,
            cache,
            persistence,
            upstream,
            snapshot,
            channels,
            cipher,
        );
        (state, credential, dir)
    }

    /// Read the sealed secret currently stored for `cred`.
    async fn stored_secret(state: &AppState, cred: &Credential) -> Value {
        state
            .persistence
            .list_credentials(cred.provider_id)
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
        let (state, cred, _dir) =
            state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
        let refreshes = Arc::new(AtomicUsize::new(0));
        let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
            refreshes: refreshes.clone(),
            sleep_ms: 0,
        });

        let got = state
            .refresh
            .ensure_fresh(
                &state,
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
        let (state, cred, _dir) =
            state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
        let refreshes = Arc::new(AtomicUsize::new(0));
        let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
            refreshes: refreshes.clone(),
            sleep_ms: 50,
        });
        let provider = test_provider();

        let (result, _) = tokio::join!(
            state.refresh.ensure_fresh(
                &state,
                &channel,
                &cred,
                &provider,
                json!({"access_token": "old"}),
                false,
            ),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                state
                    .persistence
                    .delete_credential(cred.id)
                    .await
                    .expect("delete credential");
            },
        );

        assert!(result.is_err(), "refresh must not use a deleted credential");
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        assert!(
            state
                .persistence
                .get_credential(cred.id)
                .await
                .unwrap()
                .is_none(),
            "refresh writeback must not reinsert the credential"
        );
    }

    #[tokio::test]
    async fn refresh_does_not_reenable_disabled_credential() {
        let cipher = cipher();
        let (state, cred, _dir) =
            state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
        let refreshes = Arc::new(AtomicUsize::new(0));
        let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
            refreshes: refreshes.clone(),
            sleep_ms: 50,
        });
        let provider = test_provider();
        let original_secret = cred.secret_json.clone();

        let (result, _) = tokio::join!(
            state.refresh.ensure_fresh(
                &state,
                &channel,
                &cred,
                &provider,
                json!({"access_token": "old"}),
                false,
            ),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                state
                    .persistence
                    .upsert_credential(CredentialInput {
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
                    })
                    .await
                    .expect("disable credential");
            },
        );

        assert!(
            result.is_err(),
            "refresh must not use a disabled credential"
        );
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        let stored = state
            .persistence
            .get_credential(cred.id)
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
            .refresh
            .ensure_fresh(
                &state,
                &channel,
                &cred,
                &test_provider(),
                fresh.clone(),
                false,
            )
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
        let (state, cred, _dir) =
            state_with_cred(cipher.clone(), json!({"access_token": "old"})).await;
        let refreshes = Arc::new(AtomicUsize::new(0));
        let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
            refreshes: refreshes.clone(),
            sleep_ms: 20,
        });

        let stale = json!({"access_token": "old"});
        let provider = test_provider();
        let (a, b) = tokio::join!(
            state
                .refresh
                .ensure_fresh(&state, &channel, &cred, &provider, stale.clone(), false),
            state
                .refresh
                .ensure_fresh(&state, &channel, &cred, &provider, stale.clone(), false),
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
            state
                .refresh
                .ensure_fresh(&state, &channel, &cred, &provider, orig.clone(), true),
            state
                .refresh
                .ensure_fresh(&state, &channel, &cred, &provider, orig.clone(), true),
        );

        // Exactly one rotation; both callers see the same rotated token.
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
        assert_eq!(a.unwrap(), b.unwrap());
    }
}
