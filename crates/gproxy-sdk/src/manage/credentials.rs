//! Credentials: the sealed material a provider authenticates with, and the
//! account operations around it.
//!
//! The secret never leaves this family in the clear except through
//! `reveal_secret`, which is one deliberate call rather than a field on every
//! read. Everything else about a credential — its label, auth kind, metadata,
//! connection profile and owner — is frozen into the execution snapshot at
//! assembly, which is why only a write limited to the secret, its expiry and
//! the lifecycle status can take the cheap [`Scope::CredentialState`] path.

use gproxy_channel::channel::{CredentialContext, CredentialView, ProviderView};
use gproxy_core::{RefreshMode, keys};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{
    Repository,
    entity::{
        limits::{credential_block, credential_quota_cycle},
        upstream::{
            credential::{self, CredentialStatus},
            provider,
        },
    },
    operations::credentials::CredentialStatusUpdate,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Select, Set, sea_query::Expr};
use serde_json::Value;

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, CredentialBlockDto, CredentialCycleDto, CredentialDto, CredentialLimitStatusDto,
        CredentialPatch, CredentialQuotaDto, CredentialSummaryDto, CredentialWrite, ListQuery,
        Page, QuotaResetDto, QuotaSnapshotDto,
    },
};

/// A CAS loop that cannot outlive one management call. A key contended this
/// many times is a peer writing continuously, and the next reload fixes it.
const CAS_ATTEMPTS: usize = 8;

pub struct Credentials<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Credentials<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Credentials<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<CredentialDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<CredentialDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: CredentialWrite) -> SdkResult<CredentialDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: CredentialPatch) -> SdkResult<CredentialDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<CredentialWrite, CredentialPatch>>,
    ) -> SdkResult<Vec<Option<CredentialDto>>> {
        crud::batch(self, items).await
    }

    /// The plaintext secret. Separate from `get` on purpose: a management UI
    /// lists credentials constantly and reveals one rarely, and only that rare
    /// call needs to be authorized and audited as the disclosure it is.
    pub async fn reveal_secret(&self, id: &str) -> SdkResult<Value> {
        let row = crud::row::<C, Self>(self, id).await?;
        Ok(self
            .writer
            .core()
            .secret_codec()
            .open(&row.id, &row.secret)?)
    }

    /// Retire a credential or bring it back. Version-checked against a
    /// concurrent refresh, so an operator cannot mark Dead a credential that
    /// was rotated a moment ago. The empty revision commit that follows is
    /// what tells this instance and its peers to re-read it.
    pub async fn set_status(
        &self,
        id: &str,
        status: CredentialStatus,
        reason: Option<String>,
    ) -> SdkResult<CredentialDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        let outcome = self
            .writer
            .store()
            .credentials()
            .set_status_many(vec![CredentialStatusUpdate {
                id: row.id.clone(),
                expected_version: row.version,
                status,
                reason: crud::optional_text(reason),
            }])
            .await?
            .into_iter()
            .next()
            .ok_or(gproxy_store::StoreError::UnexpectedResult)?;
        if outcome == gproxy_store::operations::CasOutcome::Conflict {
            return Err(SdkError::conflict(format!(
                "credential `{id}` changed while its status was being written"
            )));
        }
        self.writer
            .commit(Vec::new(), &[Scope::CredentialState(vec![id.to_owned()])])
            .await?;
        crud::get(self, id).await
    }

    /// Renew the material through the channel. Core owns the lease, the CAS
    /// and the publication; this only names the credential.
    pub async fn refresh(&self, id: &str, mode: RefreshMode) -> SdkResult<CredentialSummaryDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        Ok(self
            .writer
            .core()
            .refresh_credential(&row.provider_id, &row.id, mode)
            .await?
            .into())
    }

    /// Ask the upstream what this account has left. The answer is persisted as
    /// cycles and blocks before it is returned, so a probe is also how a
    /// credential comes back into selection after an observed reset.
    pub async fn quota_probe(&self, id: &str) -> SdkResult<QuotaSnapshotDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        Ok(self
            .writer
            .core()
            .query_credential_quota(&row.provider_id, &row.id)
            .await?
            .into())
    }

    /// What this deployment already knows, without asking the upstream: the
    /// observed cycles, newest first, and the blocks still in force.
    pub async fn quota_read(&self, id: &str) -> SdkResult<CredentialQuotaDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        let cycles = self
            .writer
            .store()
            .credential_quota_cycles()
            .query(
                credential_quota_cycle::Entity::find()
                    .filter(credential_quota_cycle::Column::CredentialId.eq(&row.id))
                    .order_by_desc(credential_quota_cycle::Column::ObservedAtMs),
            )
            .await?;
        let now = crate::rt::now_ms();
        let blocks = self
            .writer
            .store()
            .credential_blocks()
            .query(
                credential_block::Entity::find()
                    .filter(credential_block::Column::CredentialId.eq(&row.id))
                    .filter(credential_block::Column::UntilMs.gt(now))
                    .order_by_asc(credential_block::Column::UntilMs),
            )
            .await?;
        Ok(CredentialQuotaDto {
            cycles: cycles.into_iter().map(CredentialCycleDto::from).collect(),
            blocks: blocks.into_iter().map(CredentialBlockDto::from).collect(),
        })
    }

    /// Redeem an upstream reset credit, where the channel offers one. Most do
    /// not: reporting a window is not the same as being able to reopen it.
    pub async fn quota_reset(&self, id: &str) -> SdkResult<QuotaResetDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        let provider = self
            .writer
            .store()
            .providers()
            .get_many(std::slice::from_ref(&row.provider_id))
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("provider", row.provider_id.clone()))?;
        let channel = self
            .writer
            .core()
            .channels()
            .get(&provider.channel)
            .cloned()
            .ok_or_else(|| {
                SdkError::invalid(format!(
                    "channel `{}` is not registered in this build",
                    provider.channel
                ))
            })?;
        let Some(reset) = channel.quota_reset() else {
            return Err(SdkError::Unsupported(
                "this channel cannot reopen a spent quota window",
            ));
        };
        let secret = self
            .writer
            .core()
            .secret_codec()
            .open(&row.id, &row.secret)?;
        let client = self.writer.core().provider_client(&provider.id).await?;
        let redeem_request_id = crate::ids::random_id();
        let result = reset
            .reset(
                CredentialContext {
                    provider: view(&provider),
                    credential: CredentialView {
                        id: &row.id,
                        provider_id: &row.provider_id,
                        auth_kind: &row.auth_kind,
                        secret: &secret,
                        metadata: &row.metadata,
                        version: row.version,
                        expires_at_ms: row.expires_at_ms,
                    },
                    client: client.as_ref(),
                },
                &redeem_request_id,
            )
            .await?;
        Ok(result.into())
    }

    /// Clear everything keeping a credential out of selection: its persisted
    /// blocks, the cache's hot copy of them, and a Dead status. One commit for
    /// the rows, then a compare-and-swap for the cache — in that order, so a
    /// peer that reads the cache after the delete cannot re-warm a block that
    /// no longer exists durably.
    pub async fn health_reset(&self, id: &str) -> SdkResult<CredentialDto> {
        let row = crud::row::<C, Self>(self, id).await?;
        let mut statements = vec![BatchStatement::Execute(
            self.writer
                .store()
                .credential_blocks()
                .delete_where_statement(
                    credential_block::Entity::delete_many()
                        .filter(credential_block::Column::CredentialId.eq(&row.id)),
                ),
        )];
        if row.status != CredentialStatus::Active {
            statements.push(BatchStatement::Execute(
                self.writer.store().credentials().update_where_statement(
                    credential::Entity::update_many()
                        .col_expr(
                            credential::Column::Status,
                            Expr::val(CredentialStatus::Active),
                        )
                        .col_expr(credential::Column::StatusReason, Expr::val(None::<String>))
                        .col_expr(
                            credential::Column::Version,
                            Expr::val(row.version.saturating_add(1)),
                        )
                        .filter(credential::Column::Id.eq(&row.id))
                        .filter(credential::Column::Version.eq(row.version)),
                ),
            ));
        }
        self.writer
            .commit(statements, &[Scope::CredentialState(vec![row.id.clone()])])
            .await?;
        self.clear_cached_blocks(&row.provider_id, &row.id).await?;
        crud::get(self, id).await
    }

    /// Every operator limit covering this credential, with its usage.
    pub async fn limit_status(&self, id: &str) -> SdkResult<Vec<CredentialLimitStatusDto>> {
        Ok(self
            .writer
            .core()
            .credential_limit_status(id, crate::rt::now_ms())
            .await?
            .into_iter()
            .map(CredentialLimitStatusDto::from)
            .collect())
    }

    async fn clear_cached_blocks(&self, provider_id: &str, credential_id: &str) -> SdkResult<()> {
        let key = keys::credential_blocks(provider_id, credential_id);
        let cache = self.writer.inner().cache.clone();
        for _ in 0..CAS_ATTEMPTS {
            let Some(entry) = cache.get(&key).await? else {
                return Ok(());
            };
            if cache
                .compare_exchange(&key, Some(entry.version), None)
                .await?
                != gproxy_cache::CasOutcome::Conflict
            {
                return Ok(());
            }
        }
        // The durable rows are gone; the stale cache copy expires on its own
        // TTL and the next reload re-warms whatever is actually there.
        tracing::warn!(credential_id, "cached credential blocks stayed contended");
        Ok(())
    }

    async fn provider(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "providerId")?;
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }

    async fn profile(&self, id: Option<String>) -> SdkResult<Option<String>> {
        let Some(id) = crud::optional_text(id) else {
            return Ok(None);
        };
        crud::require_rows(
            self.writer.store().connection_profiles(),
            "connection profile",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(Some(id))
    }

    fn seal(&self, id: &str, secret: &Value) -> SdkResult<Vec<u8>> {
        if secret.is_null() {
            return Err(SdkError::invalid("secret must not be null"));
        }
        Ok(self.writer.core().secret_codec().seal(id, secret)?)
    }
}

fn view(provider: &provider::Model) -> ProviderView<'_> {
    ProviderView {
        id: &provider.id,
        channel: &provider.channel,
        base_url: provider.base_url.as_deref(),
        config: &provider.config,
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Credentials<'_, C> {
    type Entity = credential::Entity;
    type Dto = CredentialDto;
    type Write = CredentialWrite;
    type Patch = CredentialPatch;

    const ENTITY: &'static str = "credential";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().credentials()
    }
    fn scopes(&self) -> Vec<Scope> {
        // A create or a delete changes which credentials exist, which only a
        // full reload can publish.
        vec![Scope::Credentials(Vec::new())]
    }
    fn update_scopes(&self, id: &str, patch: &CredentialPatch) -> Vec<Scope> {
        if patch.state_only() {
            vec![Scope::CredentialState(vec![id.to_owned()])]
        } else {
            vec![Scope::Credentials(vec![id.to_owned()])]
        }
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = credential::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(credential::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(credential::Column::Label.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(credential::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: CredentialWrite) -> SdkResult<(credential::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = credential::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(self.provider(&write.provider_id).await?),
            organization_id: Set(crud::optional_text(write.organization_id)),
            team_id: Set(crud::optional_text(write.team_id)),
            user_id: Set(crud::optional_text(write.user_id)),
            label: Set(crud::optional_text(write.label)),
            auth_kind: Set(crud::text(&write.auth_kind, "authKind")?),
            secret: Set(self.seal(&id, &write.secret)?),
            version: Set(0),
            connection_profile_id: Set(self.profile(write.connection_profile_id).await?),
            metadata: Set(crud::object(write.metadata, "metadata")?),
            expires_at_ms: Set(write.expires_at_ms),
            status: Set(CredentialStatus::Active),
            status_reason: Set(None),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &credential::Model,
        patch: CredentialPatch,
    ) -> SdkResult<credential::ActiveModel> {
        let mut row = credential::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(label) = patch.label {
            row.label = Set(crud::optional_text(label));
        }
        if let Some(auth_kind) = patch.auth_kind {
            row.auth_kind = Set(crud::text(&auth_kind, "authKind")?);
        }
        if let Some(secret) = patch.secret {
            // New material is a new version, exactly as a refresh would write
            // it: that is what makes peers re-read the row.
            row.secret = Set(self.seal(&current.id, &secret)?);
            row.version = Set(current.version.saturating_add(1));
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        if let Some(metadata) = patch.metadata {
            row.metadata = Set(crud::object(Some(metadata), "metadata")?);
        }
        if let Some(profile) = patch.connection_profile_id {
            row.connection_profile_id = Set(self.profile(profile).await?);
        }
        if let Some(expires_at_ms) = patch.expires_at_ms {
            row.expires_at_ms = Set(expires_at_ms);
        }
        if let Some(organization_id) = patch.organization_id {
            row.organization_id = Set(crud::optional_text(organization_id));
        }
        if let Some(team_id) = patch.team_id {
            row.team_id = Set(crud::optional_text(team_id));
        }
        if let Some(user_id) = patch.user_id {
            row.user_id = Set(crud::optional_text(user_id));
        }
        Ok(row)
    }
}
