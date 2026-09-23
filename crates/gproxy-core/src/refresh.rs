//! Explicit credential refresh: one lease per credential across instances,
//! the channel's `CredentialRefresh` as the only source of new material, and
//! version-CAS persistence before publication. A peer's newer durable version
//! satisfies `IfNeeded`; a definitive rejection persists `Dead` with its
//! reason and is surfaced as `CredentialDead`.

use crate::{
    Core, CoreError, CoreResult, CredentialData, CredentialStatus, CredentialSummary,
    CredentialVersion, Invalidation, RefreshMode, api::lifecycle::now_ms, keys,
};
use gproxy_channel::{
    ChannelError,
    channel::{CredentialContext, CredentialView},
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    entity::upstream::credential,
    operations::{
        CasOutcome,
        credentials::{CredentialRefresh as RefreshRow, CredentialStatusUpdate},
    },
};
use std::{sync::Arc, time::Duration};

/// A refresh that runs longer than this lost its lease; peers may start one.
const LEASE_TTL: Duration = Duration::from_secs(30);
/// Waiting for a peer's refresh: poll interval and total patience.
const LEASE_POLL: Duration = Duration::from_millis(100);
const LEASE_WAIT: Duration = Duration::from_secs(10);
/// `IfNeeded` refreshes when the material expires within this margin.
pub const REFRESH_MARGIN_MS: i64 = 60_000;

/// Whether `IfNeeded` should act on this version now.
pub(crate) fn needs_refresh(version: &CredentialVersion, now_ms: i64) -> bool {
    version
        .expires_at_ms
        .is_some_and(|expires| expires <= now_ms + REFRESH_MARGIN_MS)
}

fn summary(credential_id: &str, version: &CredentialVersion) -> CredentialSummary {
    CredentialSummary {
        credential_id: credential_id.to_owned(),
        version: version.version,
        expires_at_ms: version.expires_at_ms,
        status: version.status,
        status_reason: version.status_reason.clone(),
    }
}

impl<C: BatchConnectionTrait + Send + Sync> Core<C> {
    /// Explicit account operation, not an internal attempt hook. Validates
    /// provider ownership, coordinates a shared refresh lease, reads current
    /// Store material, invokes the channel capability, then seals and
    /// CAS-persists before publishing. A peer's newer durable version may
    /// satisfy `IfNeeded`. A definitive rejection is persisted as Dead with its
    /// reason and returned as `CredentialDead`; a Dead credential is not
    /// refreshed again, even with `Force`. The upper layer must authorize.
    pub async fn refresh_credential(
        &self,
        provider_id: &str,
        credential_id: &str,
        mode: RefreshMode,
    ) -> CoreResult<CredentialSummary> {
        let snapshot = self.snapshot();
        let credential = snapshot
            .credentials
            .get(credential_id)
            .filter(|c| c.provider_id == provider_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!(
                    "credential `{credential_id}` is not part of provider `{provider_id}`"
                ))
            })?;
        let provider = snapshot
            .providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!("provider `{provider_id}` is not loaded"))
            })?;
        let current = credential.state.load();
        if current.status == CredentialStatus::Dead {
            return Err(dead(credential_id, &current));
        }
        let now = now_ms();
        if mode == RefreshMode::IfNeeded && !needs_refresh(&current, now) {
            return Ok(summary(credential_id, &current));
        }
        let Some(refresher) = provider.channel.credential_refresh() else {
            return Err(CoreError::Channel(ChannelError::UnsupportedService));
        };

        // One refresher per credential across instances. While waiting, a peer
        // may finish first; its durable version then answers this call.
        let lease_key = keys::refresh_lease(credential_id);
        let waited_from = web_time::Instant::now();
        let lease = loop {
            if let Some(lease) = self.cache().acquire_lease(&lease_key, LEASE_TTL).await? {
                break lease;
            }
            if let Some(row) = self.read_row(credential_id).await?
                && row.version > current.version
            {
                let published = self.publish_row(&credential, &row)?;
                return finish_peer(credential_id, &published);
            }
            if waited_from.elapsed() >= LEASE_WAIT {
                return Err(CoreError::RefreshContended {
                    credential_id: credential_id.to_owned(),
                });
            }
            crate::rt::sleep(LEASE_POLL).await;
        };
        let result = self
            .refresh_under_lease(&provider, &credential, &current, mode, refresher)
            .await;
        let _ = self.cache().release_permit(&lease_key, lease).await;
        result
    }

    async fn refresh_under_lease(
        &self,
        provider: &crate::ProviderData,
        credential: &Arc<CredentialData>,
        current: &Arc<CredentialVersion>,
        mode: RefreshMode,
        refresher: &dyn gproxy_channel::channel::CredentialRefresh,
    ) -> CoreResult<CredentialSummary> {
        let credential_id = credential.id.as_str();
        // Authoritative material: a peer may have rotated since our snapshot.
        let Some(row) = self.read_row(credential_id).await? else {
            credential.state.retire();
            return Err(CoreError::InvalidTarget(format!(
                "credential `{credential_id}` no longer exists"
            )));
        };
        let row_version = self.publish_row(credential, &row)?;
        if row_version.status == CredentialStatus::Dead {
            return Err(dead(credential_id, &row_version));
        }
        if row.version > current.version {
            // A peer refreshed while we waited for the lease.
            return finish_peer(credential_id, &row_version);
        }
        if mode == RefreshMode::IfNeeded && !needs_refresh(&row_version, now_ms()) {
            return Ok(summary(credential_id, &row_version));
        }

        let mut context = CredentialContext {
            provider: crate::assemble::provider_view(&provider.entity),
            credential: CredentialView {
                id: credential_id,
                provider_id: &credential.provider_id,
                auth_kind: &credential.auth_kind,
                secret: &row_version.secret,
                metadata: &credential.metadata,
                version: row_version.version,
                expires_at_ms: row_version.expires_at_ms,
            },
            client: credential.client.as_ref(),
        };
        let cookie_client = if refresher.connection_purpose(&context.credential)
            == gproxy_channel::channel::ConnectionPurpose::CookieLogin
            && row.connection_profile_id.is_none()
        {
            Some(
                self.provider_client_for_proxy(
                    &provider.entity.id,
                    gproxy_channel::channel::ConnectionPurpose::CookieLogin,
                    row.proxy.as_ref(),
                )
                .await?,
            )
        } else {
            None
        };
        if let Some(client) = &cookie_client {
            context.client = client.as_ref();
        }
        let update = match refresher.refresh(context).await {
            Ok(update) => update,
            Err(ChannelError::RefreshRejected(reason)) => {
                return Err(self
                    .mark_dead(credential, &row_version, reason)
                    .await
                    .unwrap_or_else(|error| error));
            }
            Err(error) => return Err(CoreError::Channel(error)),
        };

        let sealed = self
            .secret_codec()
            .seal(credential_id, &update.secret)
            .map_err(CoreError::Secret)?;
        let outcome = self
            .store()
            .credentials()
            .refresh_many(vec![RefreshRow {
                id: credential_id.to_owned(),
                expected_version: row.version,
                secret: sealed,
                expires_at_ms: update.expires_at_ms,
            }])
            .await?;
        match outcome.first() {
            Some(CasOutcome::Applied) => {
                let next = Arc::new(CredentialVersion {
                    version: row.version + 1,
                    expires_at_ms: update.expires_at_ms,
                    secret: update.secret,
                    status: CredentialStatus::Active,
                    status_reason: None,
                });
                credential.state.publish_if_newer(next.clone());
                self.notify_changed(credential_id, next.version).await;
                Ok(summary(credential_id, &next))
            }
            _ => {
                // Lost the race after all: the durable row wins.
                match self.read_row(credential_id).await? {
                    Some(row) => {
                        let published = self.publish_row(credential, &row)?;
                        finish_peer(credential_id, &published)
                    }
                    None => {
                        credential.state.retire();
                        Err(CoreError::InvalidTarget(format!(
                            "credential `{credential_id}` no longer exists"
                        )))
                    }
                }
            }
        }
    }

    /// Persist Dead by version CAS and publish it. Returns the error to hand
    /// back: `CredentialDead` on success, or a peer's state if the CAS lost.
    async fn mark_dead(
        &self,
        credential: &Arc<CredentialData>,
        current: &CredentialVersion,
        reason: String,
    ) -> CoreResult<CoreError> {
        let credential_id = credential.id.as_str();
        let outcome = self
            .store()
            .credentials()
            .set_status_many(vec![CredentialStatusUpdate {
                id: credential_id.to_owned(),
                expected_version: current.version,
                status: CredentialStatus::Dead,
                reason: Some(reason.clone()),
            }])
            .await?;
        match outcome.first() {
            Some(CasOutcome::Applied) => {
                let next = Arc::new(CredentialVersion {
                    version: current.version + 1,
                    expires_at_ms: current.expires_at_ms,
                    secret: current.secret.clone(),
                    status: CredentialStatus::Dead,
                    status_reason: Some(reason),
                });
                credential.state.publish_if_newer(next.clone());
                self.notify_changed(credential_id, next.version).await;
                Ok(dead(credential_id, &next))
            }
            _ => match self.read_row(credential_id).await? {
                Some(row) => {
                    let published = self.publish_row(credential, &row)?;
                    Ok(match finish_peer(credential_id, &published) {
                        Ok(_) => dead(credential_id, &published),
                        Err(error) => error,
                    })
                }
                None => {
                    credential.state.retire();
                    Ok(CoreError::InvalidTarget(format!(
                        "credential `{credential_id}` no longer exists"
                    )))
                }
            },
        }
    }

    async fn read_row(&self, credential_id: &str) -> CoreResult<Option<credential::Model>> {
        Ok(self
            .store()
            .credentials()
            .get_many(&[credential_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten())
    }

    /// Open a Store row and publish it into the live slot if newer.
    fn publish_row(
        &self,
        credential: &CredentialData,
        row: &credential::Model,
    ) -> CoreResult<Arc<CredentialVersion>> {
        let secret = self
            .secret_codec()
            .open(&row.id, &row.secret)
            .map_err(CoreError::Secret)?;
        let version = Arc::new(CredentialVersion {
            version: row.version,
            expires_at_ms: row.expires_at_ms,
            secret,
            status: row.status,
            status_reason: row.status_reason.clone(),
        });
        credential.state.publish_if_newer(version.clone());
        Ok(credential.state.load())
    }

    async fn notify_changed(&self, credential_id: &str, version: i64) {
        let payload = serde_json::to_vec(&Invalidation::CredentialChanged {
            credential_id: credential_id.to_owned(),
            version,
        })
        .unwrap_or_default();
        // Best effort: peers reconcile with the Store on resync anyway.
        let _ = self
            .cache()
            .publish(keys::INVALIDATION_TOPIC, payload)
            .await;
    }
}

fn dead(credential_id: &str, version: &CredentialVersion) -> CoreError {
    CoreError::CredentialDead {
        credential_id: credential_id.to_owned(),
        reason: version.status_reason.clone(),
    }
}

/// A peer's durable version stands in for our refresh, unless it is Dead.
fn finish_peer(credential_id: &str, version: &CredentialVersion) -> CoreResult<CredentialSummary> {
    if version.status == CredentialStatus::Dead {
        return Err(dead(credential_id, version));
    }
    Ok(summary(credential_id, version))
}
