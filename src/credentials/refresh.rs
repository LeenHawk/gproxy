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

use serde_json::Value;

mod locks;

use locks::RefreshLocks;

use crate::channel::{Channel, ChannelError};
use crate::crypto::SecretCipher;
use crate::http::client::{ClientError, UpstreamClient};
use crate::store::cache::CacheBackend;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::Credential;

/// Services used by a refresh, supplied by the application layer. Client
/// resolution stays lazy so single-flight losers can reuse the winner without
/// failing on or constructing an upstream client they no longer need.
pub struct RefreshDeps<'a> {
    pub persistence: &'a dyn PersistenceBackend,
    pub cache: &'a dyn CacheBackend,
    pub cipher: &'a dyn SecretCipher,
    pub resolve_client:
        &'a (dyn Fn() -> Result<Arc<dyn UpstreamClient>, ClientError> + Send + Sync),
}

/// Serialises refreshes per credential id so concurrent requests cannot rotate
/// the same credential twice.
pub struct RefreshOrchestrator {
    locks: RefreshLocks,
}

impl Default for RefreshOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl RefreshOrchestrator {
    pub fn new() -> Self {
        Self {
            locks: RefreshLocks::new(),
        }
    }

    /// Return a fresh decrypted secret for the credential, refreshing if the
    /// channel deems it stale. `opened` is the already-decrypted current secret.
    /// `force` skips the staleness gate (AuthDead-triggered forced refresh).
    pub async fn ensure_fresh(
        &self,
        deps: RefreshDeps<'_>,
        channel: &Arc<dyn Channel>,
        credential: &Credential,
        opened: Value,
        force: bool,
    ) -> Result<Value, ChannelError> {
        if !force && !channel.needs_refresh(&opened) {
            return Ok(opened);
        }
        let lock = self.locks.for_credential(credential.id);
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
        let mut current = reread_open_enabled(deps.persistence, deps.cipher, credential)
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
        // (cool + skip), never a silent downgrade to the default client.
        let client = (deps.resolve_client)()
            .map_err(|e| ChannelError::Build(format!("resolve refresh client: {e}")))?;
        // Cross-instance single-flight: the local mutex above serialises this
        // instance, but a single-use refresh_token must not be rotated by two
        // instances at once. Acquire a best-effort redis lock around the actual
        // upstream refresh. Default-true on memory/edge, so single-instance and
        // wasm builds take the fast path (always `acquired`).
        let lock_key = format!("gproxy:refresh:lock:{}", credential.id);
        let acquired = deps
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
            let peer = reread_open_enabled(deps.persistence, deps.cipher, credential)
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
            deps.cache.unlock(&lock_key).await;
        }
        let fresh = fresh?;
        // seal + writeback + publish — channel error already propagated above so
        // the caller cools + skips the credential on a failed refresh.
        let sealed = deps
            .cipher
            .seal(&fresh)
            .map_err(|e| ChannelError::Build(format!("seal refreshed secret: {e}")))?;
        writeback(deps.persistence, credential, current.updated_at, sealed)
            .await
            .map_err(|e| ChannelError::Build(format!("persist refreshed secret: {e}")))?;
        crate::store::cache::broadcast(deps.cache, format!("cred:{}", credential.id).as_bytes())
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
    persistence: &dyn PersistenceBackend,
    cipher: &dyn SecretCipher,
    credential: &Credential,
) -> anyhow::Result<Option<OpenCredential>> {
    let Some(stored) = PersistenceBackend::get_credential(persistence, credential.id).await? else {
        return Ok(None);
    };
    if stored.provider_id != credential.provider_id || !stored.enabled {
        return Ok(None);
    }
    Ok(Some(OpenCredential {
        secret: cipher.open(&stored.secret_json)?,
        updated_at: stored.updated_at,
    }))
}

/// Persist only the re-sealed secret. This is deliberately not a generic
/// upsert: it must not insert a deleted credential, re-enable a disabled one,
/// or overwrite any admin-edited fields from the stale snapshot.
async fn writeback(
    persistence: &dyn PersistenceBackend,
    credential: &Credential,
    expected_updated_at: i64,
    sealed: Value,
) -> anyhow::Result<()> {
    let updated = PersistenceBackend::update_credential_secret_if_current(
        persistence,
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
mod tests;
