//! Single-flight OAuth credential refresh (§14.5). Lazy: only when the channel
//! says the decrypted secret needs it. Per-credential local mutex serialises
//! concurrent refreshes within an instance (many providers rotate refresh_token
//! each call, so a double refresh kills the credential). Across instances, a
//! distributed lease (key = `gproxy:refresh:lock:{cred id}`) spans rotation and
//! CAS writeback so two instances cannot spend a single-use token concurrently;
//! the loser waits, re-reads, and reuses the winner's result. Redis, Upstash, and
//! libSQL provide owner-scoped leases; memory relies on the local mutex.
//!
//! The mutex is `futures_util::lock::Mutex` (runtime-agnostic): tokio is a
//! native-only dependency, so the edge/wasm build cannot use `tokio::sync`.

use std::sync::Arc;

use futures_util::future::Either;
use serde_json::Value;

mod locks;

use locks::RefreshLocks;

use crate::channel::{Channel, ChannelError, RefreshCtx};
use crate::crypto::SecretCipher;
use crate::http::client::{ClientError, UpstreamClient};
use crate::store::cache::{CacheBackend, LockAttempt};
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::Credential;

const DISTRIBUTED_LOCK_TTL: std::time::Duration = std::time::Duration::from_secs(60);
const DISTRIBUTED_LOCK_RETRIES: usize = 60;
const COMPROMISED_RESULT_RETRIES: usize = 30;
#[cfg(not(test))]
const DISTRIBUTED_LOCK_HEARTBEAT_MS: u64 = 5_000;
#[cfg(test)]
const DISTRIBUTED_LOCK_HEARTBEAT_MS: u64 = 1;
#[cfg(not(test))]
const DISTRIBUTED_LOCK_RETRY_MS: u64 = 1_000;
#[cfg(test)]
const DISTRIBUTED_LOCK_RETRY_MS: u64 = 1;

/// Lazily resolves the transport from the latest decrypted secret under lock.
pub type RefreshClientResolver<'a> =
    dyn Fn(&Value) -> Result<Arc<dyn UpstreamClient>, ClientError> + Send + Sync + 'a;

/// Services used by a refresh, supplied by the application layer. Client
/// resolution stays lazy so single-flight losers can reuse the winner without
/// failing on or constructing an upstream client they no longer need.
pub struct RefreshDeps<'a> {
    pub persistence: &'a dyn PersistenceBackend,
    pub cache: &'a dyn CacheBackend,
    pub cipher: &'a dyn SecretCipher,
    pub provider_settings: &'a Value,
    pub resolve_client: &'a RefreshClientResolver<'a>,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub disable_log_redaction: bool,
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
        let mode = if force { "forced" } else { "lazy" };
        tracing::debug!(
            credential_id = credential.id,
            channel = channel.id(),
            mode,
            "credential.refresh.started"
        );
        let result = self
            .ensure_fresh_inner(deps, channel, credential, opened, force)
            .await;
        match &result {
            Ok(_) => tracing::debug!(
                credential_id = credential.id,
                channel = channel.id(),
                mode,
                "credential.refresh.succeeded"
            ),
            Err(error) => {
                tracing::warn!(
                    credential_id = credential.id,
                    channel = channel.id(),
                    mode,
                    error_kind = channel_error_kind(error),
                    "credential.refresh.failed"
                );
            }
        }
        result
    }

    async fn ensure_fresh_inner(
        &self,
        deps: RefreshDeps<'_>,
        channel: &Arc<dyn Channel>,
        credential: &Credential,
        opened: Value,
        force: bool,
    ) -> Result<Value, ChannelError> {
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
        let current = reread_open_enabled(deps.persistence, deps.cipher, credential)
            .await
            .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
            .ok_or_else(|| {
                ChannelError::Build("credential changed or disabled during refresh".into())
            })?;
        if !force && !channel.needs_refresh(&current.secret) {
            peer_reused(credential, channel, "local_single_flight");
            return Ok(current.secret);
        }
        if force && current.secret != opened {
            peer_reused(credential, channel, "local_single_flight");
            return Ok(current.secret);
        }
        // Cross-instance single-flight: the local mutex above serialises this
        // instance, but a single-use refresh_token must not be rotated by two
        // instances at once. The lease spans both the upstream rotation and its
        // CAS writeback; releasing before persistence would let a peer re-read
        // and spend the old refresh token again.
        let lock_key = format!("gproxy:refresh:lock:{}", credential.id);
        let lock_owner = crate::util::rand::uuid_v4();
        let mut acquired = false;
        for attempt in 0..=DISTRIBUTED_LOCK_RETRIES {
            match deps
                .cache
                .try_lock(&lock_key, &lock_owner, DISTRIBUTED_LOCK_TTL)
                .await
            {
                LockAttempt::Acquired => {
                    acquired = true;
                    break;
                }
                LockAttempt::Unavailable => {
                    return Err(ChannelError::Transient(
                        "credential refresh lock backend unavailable".into(),
                    ));
                }
                LockAttempt::Busy => {}
            }
            if attempt == DISTRIBUTED_LOCK_RETRIES {
                break;
            }
            crate::util::time::sleep_ms(DISTRIBUTED_LOCK_RETRY_MS).await;
            let peer = reread_open_enabled(deps.persistence, deps.cipher, credential)
                .await
                .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
                .ok_or_else(|| {
                    ChannelError::Build("credential changed or disabled during refresh".into())
                })?;
            if (!force && !channel.needs_refresh(&peer.secret)) || (force && peer.secret != opened)
            {
                peer_reused(credential, channel, "distributed_wait");
                return Ok(peer.secret);
            }
        }
        if !acquired {
            return Err(ChannelError::Transient(
                "credential refresh lock remained busy".into(),
            ));
        }

        let result = refresh_under_lease(
            &deps,
            channel,
            credential,
            &opened,
            force,
            &lock_key,
            &lock_owner,
        )
        .await;
        deps.cache.unlock(&lock_key, &lock_owner).await;
        result
    }
}

async fn refresh_under_lease(
    deps: &RefreshDeps<'_>,
    channel: &Arc<dyn Channel>,
    credential: &Credential,
    opened: &Value,
    force: bool,
    lock_key: &str,
    lock_owner: &str,
) -> Result<Value, ChannelError> {
    let mut operation = Box::pin(refresh_and_persist(
        deps, channel, credential, opened, force,
    ));
    loop {
        let heartbeat = Box::pin(crate::util::time::sleep_ms(DISTRIBUTED_LOCK_HEARTBEAT_MS));
        match futures_util::future::select(operation, heartbeat).await {
            Either::Left((result, _)) => return result,
            Either::Right((_, pending)) => {
                operation = pending;
                if !deps
                    .cache
                    .extend_lock(lock_key, lock_owner, DISTRIBUTED_LOCK_TTL)
                    .await
                {
                    tracing::warn!(
                        credential_id = credential.id,
                        "credential refresh lease lost; finishing with CAS writeback"
                    );
                    let result = operation.await;
                    return recover_after_lost_lease(deps, channel, credential, opened, result)
                        .await;
                }
            }
        }
    }
}

async fn recover_after_lost_lease(
    deps: &RefreshDeps<'_>,
    channel: &Arc<dyn Channel>,
    credential: &Credential,
    opened: &Value,
    result: Result<Value, ChannelError>,
) -> Result<Value, ChannelError> {
    let error = match result {
        Ok(fresh) => return Ok(fresh),
        Err(error) => error,
    };
    for attempt in 0..=COMPROMISED_RESULT_RETRIES {
        if let Some(peer) = reread_open_enabled(deps.persistence, deps.cipher, credential)
            .await
            .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
            && peer.secret != *opened
        {
            peer_reused(credential, channel, "lost_lease_recovery");
            return Ok(peer.secret);
        }
        if attempt < COMPROMISED_RESULT_RETRIES {
            crate::util::time::sleep_ms(DISTRIBUTED_LOCK_RETRY_MS).await;
        }
    }
    Err(ChannelError::Transient(format!(
        "refresh lease was lost before a peer result appeared: {error}"
    )))
}

async fn refresh_and_persist(
    deps: &RefreshDeps<'_>,
    channel: &Arc<dyn Channel>,
    credential: &Credential,
    opened: &Value,
    force: bool,
) -> Result<Value, ChannelError> {
    let current = reread_open_enabled(deps.persistence, deps.cipher, credential)
        .await
        .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
        .ok_or_else(|| {
            ChannelError::Build("credential changed or disabled during refresh".into())
        })?;
    if (!force && !channel.needs_refresh(&current.secret)) || (force && current.secret != *opened) {
        return Ok(current.secret);
    }

    let client = (deps.resolve_client)(&current.secret)
        .map_err(|e| ChannelError::Build(format!("resolve refresh client: {e}")))?;
    let audit = super::audit::UpstreamAuditSequence::new(
        "refresh",
        deps.enable_upstream_log,
        deps.persistence,
        credential,
        deps.enable_upstream_log_body,
        deps.disable_log_redaction,
    );
    let client = audit.wrap_client(client);
    let refresh_result = channel
        .refresh(
            &client,
            RefreshCtx {
                secret: &current.secret,
                provider_settings: deps.provider_settings,
            },
        )
        .await;
    let error = refresh_result.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    let fresh = refresh_result?;

    let sealed = deps
        .cipher
        .seal(&fresh)
        .map_err(|e| ChannelError::Build(format!("seal refreshed secret: {e}")))?;
    let original_secret = current.secret.clone();
    let mut expected = current;
    for attempt in 1..=3 {
        let updated = writeback(
            deps.persistence,
            credential,
            expected.sealed.clone(),
            sealed.clone(),
        )
        .await
        .map_err(|e| ChannelError::Build(format!("persist refreshed secret: {e}")))?;
        if updated {
            crate::store::cache::broadcast(
                deps.cache,
                format!("cred:{}", credential.id).as_bytes(),
            )
            .await;
            return Ok(fresh);
        }
        tracing::debug!(
            credential_id = credential.id,
            channel = channel.id(),
            attempt,
            "credential.refresh.cas_conflict"
        );
        let Some(peer) = reread_open_enabled(deps.persistence, deps.cipher, credential)
            .await
            .map_err(|e| ChannelError::Build(format!("reread credential: {e}")))?
        else {
            return Err(ChannelError::Build(
                "credential changed or disabled during refresh".into(),
            ));
        };
        if peer.secret != original_secret {
            peer_reused(credential, channel, "cas_conflict");
            return Ok(peer.secret);
        }
        expected = peer;
    }
    Err(ChannelError::Transient(
        "credential kept changing during refresh writeback".into(),
    ))
}

fn peer_reused(credential: &Credential, channel: &Arc<dyn Channel>, source: &'static str) {
    tracing::debug!(
        credential_id = credential.id,
        channel = channel.id(),
        source,
        "credential.refresh.peer_reused"
    );
}

fn channel_error_kind(error: &ChannelError) -> &'static str {
    match error {
        ChannelError::MissingSetting(_) => "missing_setting",
        ChannelError::InvalidCredential(_) => "invalid_credential",
        ChannelError::Unsupported(_) => "unsupported",
        ChannelError::Build(_) => "build_or_persistence",
        ChannelError::Transient(_) => "transient",
    }
}

struct OpenCredential {
    secret: Value,
    sealed: Value,
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
    let sealed = stored.secret_json;
    Ok(Some(OpenCredential {
        secret: cipher.open(&sealed)?,
        sealed,
    }))
}

/// Persist only the re-sealed secret. This is deliberately not a generic
/// upsert: it must not insert a deleted credential, re-enable a disabled one,
/// or overwrite any admin-edited fields from the stale snapshot.
async fn writeback(
    persistence: &dyn PersistenceBackend,
    credential: &Credential,
    expected_secret_json: Value,
    sealed: Value,
) -> anyhow::Result<bool> {
    PersistenceBackend::update_credential_secret_if_current(
        persistence,
        credential.id,
        credential.provider_id,
        expected_secret_json,
        sealed,
    )
    .await
}

#[cfg(test)]
mod tests;
