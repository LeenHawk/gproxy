use super::{CoreError, CoreResult, CredentialStatus, RefreshMode, ReloadOutcome};
use crate::{Core, CoreData};
use gproxy_channel::channel::QuotaSnapshot;
use std::sync::Arc;

impl<C> Core<C> {
    /// Read provider execution configuration from this Core's Store and assemble
    /// a new snapshot without publishing it. Rows and durable revision must be
    /// read consistently. This includes providers, credentials, connection data,
    /// model metadata, endpoint overrides and compiled rewrite sets only.
    /// Does not load routing/identity/policy/pricing tables or run schema sync.
    /// Store loading, channel/client binding and secret-codec integration are pending.
    pub async fn load_data(&self) -> CoreResult<Arc<CoreData>> {
        Err(CoreError::NotImplemented("load_data"))
    }

    /// Load/assemble through Store, then monotonically publish. Failure leaves
    /// the previous snapshot active; existing requests retain their pinned view.
    /// Currently returns load_data's NotImplemented until assembly is implemented.
    pub async fn reload_data(&self) -> CoreResult<ReloadOutcome> {
        let next = self.load_data().await?;
        let loaded_revision = next.revision;
        let published = self.publish_snapshot(next);
        Ok(ReloadOutcome {
            loaded_revision,
            active_revision: self.snapshot().revision,
            published,
        })
    }

    /// Reload persisted credential material after a peer's update, without an
    /// upstream refresh. Preserve input order/duplicates; missing/deleted IDs
    /// return None and retire stale local slots. Never return secret contents.
    /// Caller is the trusted upper layer, responsible for authorizing IDs.
    pub async fn reload_credentials(
        &self,
        credential_ids: &[String],
    ) -> CoreResult<Vec<Option<CredentialStatus>>> {
        let _ = credential_ids;
        Err(CoreError::NotImplemented("reload_credentials"))
    }

    /// Explicit account operation, not an internal AttemptContext hook. Validate
    /// provider ownership, coordinate a shared refresh lease, read current Store
    /// material, invoke the channel capability, then seal and CAS-persist before
    /// publishing. A peer's newer durable version may satisfy IfNeeded.
    /// The upper layer must authorize this account operation.
    pub async fn refresh_credential(
        &self,
        provider_id: &str,
        credential_id: &str,
        mode: RefreshMode,
    ) -> CoreResult<CredentialStatus> {
        let _ = (provider_id, credential_id, mode);
        Err(CoreError::NotImplemented("refresh_credential"))
    }

    /// Query upstream account observations through the assigned channel/client.
    /// Does not aggregate subscription pools, change quotas or redeem reset credits.
    /// The upper layer must authorize this account operation.
    pub async fn query_credential_quota(
        &self,
        provider_id: &str,
        credential_id: &str,
    ) -> CoreResult<QuotaSnapshot> {
        let _ = (provider_id, credential_id);
        Err(CoreError::NotImplemented("query_credential_quota"))
    }
}
