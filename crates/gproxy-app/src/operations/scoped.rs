//! The scope-aware half of the configuration families.
//!
//! # Why this exists, when `admin/mod.rs` says a facade should not
//!
//! The host calls [`Gproxy::manage`] directly for every configuration family,
//! deliberately: the sdk families *are* the operation table and there is
//! nothing for a wrapper to decide. For two of them there now is. A credential
//! and a `quotas` row carry an **owner**, and who may see one is a product
//! question the sdk does not have — and must not grow, or the engine would
//! start knowing what an organization is.
//!
//! So this module is exactly the decision and nothing else: narrow a list,
//! admit a row, refuse a write that names an owner outside the scope, then
//! delegate. It is here rather than in a host because a second host binds the
//! same handle and must not invent its own answer.
//!
//! # The shape of every method
//!
//! - **a list** is narrowed through [`AdminScope::narrow`] before the query is
//!   built, so the database never runs a statement that could match a foreign
//!   row. A filter pointing outside the scope answers an empty page rather
//!   than an error;
//! - **anything taking an id** reads the row's owner first and answers
//!   `NotFound` when it is outside the scope — never `Forbidden`, which would
//!   confirm the id;
//! - **anything taking an owner** — a create, a patch that moves a row, the
//!   owner list `/quotas/status` is asked for — is refused with `Forbidden`,
//!   because the caller supplied the id rather than discovered it.
//!
//! A patch is admitted **twice**: once for the row as it is and once for the
//! row as it would be. Checking only the first would let a caller move a row
//! out of their scope; only the second would let them capture one.

use gproxy_core::BudgetOwner;
use gproxy_sdk::{
    CredentialStatus, Gproxy, RefreshMode,
    dto::{
        BatchItem, BudgetStatusDto, CredentialDto, CredentialLimitStatusDto, CredentialPatch,
        CredentialQuotaDto, CredentialSummaryDto, CredentialWrite, ListQuery, Page, QuotaDto,
        QuotaObservationDto, QuotaObservationQuery, QuotaPatch, QuotaResetDto, QuotaSnapshotDto,
        QuotaWrite,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;

use crate::{
    AdminScope, AppData, AppError, Result,
    admin_scope::{ScopeOwner, ScopedQuery},
};

/// The scope-aware configuration families, over one handle, one snapshot and
/// one scope.
///
/// Borrowed for the same reason [`Operations`](crate::Operations) is: the
/// request already holds the `Arc<AppData>` it decided under.
pub struct ScopedManage<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    scope: &'a AdminScope,
}

impl<'a, C> ScopedManage<'a, C> {
    pub fn new(gproxy: &'a Gproxy<C>, data: &'a AppData, scope: &'a AdminScope) -> Self {
        Self {
            gproxy,
            data,
            scope,
        }
    }

    pub fn credentials(&self) -> ScopedCredentials<'a, C> {
        ScopedCredentials {
            gproxy: self.gproxy,
            data: self.data,
            scope: self.scope,
        }
    }

    pub fn quotas(&self) -> ScopedQuotas<'a, C> {
        ScopedQuotas {
            gproxy: self.gproxy,
            data: self.data,
            scope: self.scope,
        }
    }
}

/// An empty page with the bounds the query asked for, for a filter that points
/// outside the scope.
fn empty<T>((offset, limit): (u64, u64)) -> Page<T> {
    Page {
        items: Vec::new(),
        total: 0,
        offset,
        limit,
    }
}

// ----------------------------------------------------------- credentials --

pub struct ScopedCredentials<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    scope: &'a AdminScope,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ScopedCredentials<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<CredentialDto>> {
        let bounds = query.bounds();
        match self.scope.narrow(query, self.data) {
            ScopedQuery::Nothing => Ok(empty(bounds)),
            ScopedQuery::Run(query) => Ok(self.manage().list(*query).await?),
        }
    }

    pub async fn get(&self, id: &str) -> Result<CredentialDto> {
        self.admit(id).await?;
        Ok(self.manage().get(id).await?)
    }

    pub async fn create(&self, write: CredentialWrite) -> Result<CredentialDto> {
        self.admit_create(&write)?;
        Ok(self.manage().create(write).await?)
    }

    pub async fn update(&self, id: &str, patch: CredentialPatch) -> Result<CredentialDto> {
        self.admit_patch(id, &patch).await?;
        Ok(self.manage().update(id, patch).await?)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.admit(id).await?;
        Ok(self.manage().delete(id).await?)
    }

    pub async fn batch(
        &self,
        items: Vec<BatchItem<CredentialWrite, CredentialPatch>>,
    ) -> Result<Vec<Option<CredentialDto>>> {
        for item in &items {
            match item {
                BatchItem::Create(write) => self.admit_create(write)?,
                BatchItem::Update(step) => self.admit_patch(&step.id, &step.patch).await?,
                BatchItem::Delete(id) => self.admit(id).await?,
            }
        }
        Ok(self.manage().batch(items).await?)
    }

    pub fn owners(&self) -> Vec<crate::dto::CredentialOwnerOptionDto> {
        let mut rows = Vec::new();
        for row in self.data.organizations.values() {
            if self
                .scope
                .admits(ScopeOwner::Organization(&row.id), self.data)
            {
                rows.push(crate::dto::CredentialOwnerOptionDto {
                    kind: "org".to_owned(),
                    id: row.id.clone(),
                    name: row.name.clone(),
                });
            }
        }
        for row in self.data.teams.values() {
            if self.scope.admits(ScopeOwner::Team(&row.id), self.data) {
                rows.push(crate::dto::CredentialOwnerOptionDto {
                    kind: "team".to_owned(),
                    id: row.id.clone(),
                    name: row.name.clone(),
                });
            }
        }
        if self.scope.is_instance() {
            for row in self.data.users.values() {
                rows.push(crate::dto::CredentialOwnerOptionDto {
                    kind: "user".to_owned(),
                    id: row.id.clone(),
                    name: row.name.clone(),
                });
            }
        }
        rows.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then(a.name.cmp(&b.name))
                .then(a.id.cmp(&b.id))
        });
        rows
    }

    pub async fn providers(&self) -> Result<Vec<crate::dto::CredentialProviderDto>> {
        // The execution snapshot excludes disabled providers. The directory
        // must still name them so their existing credentials remain editable.
        let channels = self.gproxy.channels();
        let mut rows = Vec::new();
        let mut page = 1;
        loop {
            let result = self
                .gproxy
                .manage()
                .providers()
                .list(ListQuery {
                    page: Some(page),
                    page_size: Some(500),
                    ..Default::default()
                })
                .await?;
            let done = result.items.is_empty()
                || result.offset + result.items.len() as u64 >= result.total;
            for provider in result.items {
                let descriptor = channels
                    .iter()
                    .find(|channel| channel.id == provider.channel);
                rows.push(crate::dto::CredentialProviderDto {
                    id: provider.id,
                    name: provider.name,
                    display_name: provider.display_name,
                    channel: provider.channel,
                    enabled: provider.enabled,
                    login_modes: descriptor
                        .map(|channel| channel.login_modes.clone())
                        .unwrap_or_default(),
                    capabilities: descriptor
                        .map(|channel| channel.capabilities)
                        .unwrap_or_default(),
                });
            }
            if done {
                break;
            }
            page += 1;
        }
        rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(rows)
    }

    pub async fn discover_models(
        &self,
        id: &str,
    ) -> Result<Vec<gproxy_sdk::dto::DiscoveredModelDto>> {
        let row = self.get(id).await?;
        Ok(self
            .gproxy
            .manage()
            .connectivity()
            .discover_models(&row.provider_id, Some(id))
            .await?)
    }

    pub async fn model_test(
        &self,
        id: &str,
        model: String,
    ) -> Result<gproxy_sdk::dto::ModelTestResultDto> {
        let row = self.get(id).await?;
        Ok(self
            .gproxy
            .manage()
            .connectivity()
            .model_test(gproxy_sdk::dto::ModelTest {
                provider_id: row.provider_id,
                model,
                credential_id: Some(id.to_owned()),
            })
            .await?)
    }

    pub async fn reveal_secret(&self, id: &str) -> Result<Value> {
        self.admit(id).await?;
        Ok(self.manage().reveal_secret(id).await?)
    }

    pub async fn set_status(
        &self,
        id: &str,
        status: CredentialStatus,
        reason: Option<String>,
    ) -> Result<CredentialDto> {
        self.admit(id).await?;
        Ok(self.manage().set_status(id, status, reason).await?)
    }

    pub async fn refresh(&self, id: &str, mode: RefreshMode) -> Result<CredentialSummaryDto> {
        self.admit(id).await?;
        Ok(self.manage().refresh(id, mode).await?)
    }

    pub async fn quota_read(&self, id: &str) -> Result<CredentialQuotaDto> {
        self.admit(id).await?;
        Ok(self.manage().quota_read(id).await?)
    }

    pub async fn quota_observations(
        &self,
        id: &str,
        query: QuotaObservationQuery,
    ) -> Result<Page<QuotaObservationDto>> {
        self.admit(id).await?;
        Ok(self.manage().quota_observations(id, query).await?)
    }

    pub async fn quota_probe(&self, id: &str) -> Result<QuotaSnapshotDto> {
        self.admit(id).await?;
        Ok(self.manage().quota_probe(id).await?)
    }

    pub async fn quota_reset_credits(
        &self,
        id: &str,
    ) -> Result<gproxy_sdk::dto::QuotaResetCreditsDto> {
        self.admit(id).await?;
        Ok(self.manage().quota_reset_credits(id).await?)
    }

    pub async fn quota_reset(&self, id: &str) -> Result<QuotaResetDto> {
        self.admit(id).await?;
        Ok(self.manage().quota_reset(id).await?)
    }

    pub async fn quota_reset_with(
        &self,
        id: &str,
        request: gproxy_sdk::dto::QuotaResetWrite,
    ) -> Result<QuotaResetDto> {
        self.admit(id).await?;
        Ok(self.manage().quota_reset_with(id, request).await?)
    }

    pub async fn health_reset(&self, id: &str) -> Result<CredentialDto> {
        self.admit(id).await?;
        Ok(self.manage().health_reset(id).await?)
    }

    pub async fn limit_status(&self, id: &str) -> Result<Vec<CredentialLimitStatusDto>> {
        self.admit(id).await?;
        Ok(self.manage().limit_status(id).await?)
    }

    fn manage(&self) -> gproxy_sdk::manage::Credentials<'_, C> {
        self.gproxy.manage().credentials()
    }

    /// The single row-admission call every id-taking method above starts with.
    async fn admit(&self, id: &str) -> Result<()> {
        self.scope
            .admit_credential(self.gproxy, self.data, id)
            .await
    }

    fn admit_owner(
        &self,
        user_id: Option<&str>,
        team_id: Option<&str>,
        organization_id: Option<&str>,
    ) -> Result<()> {
        self.scope.admit_write(
            ScopeOwner::from_columns(user_id, team_id, organization_id),
            self.data,
        )
    }

    fn admit_create(&self, write: &CredentialWrite) -> Result<()> {
        self.admit_transport(write.connection_profile_id.is_some(), write.proxy.is_some())?;
        self.admit_owner(
            write.user_id.as_deref(),
            write.team_id.as_deref(),
            write.organization_id.as_deref(),
        )
    }

    /// How a credential reaches its upstream — the connection profile and the
    /// outbound proxy — is instance machinery: a proxy is an address the
    /// server dials, so letting a tenant name one would let them point the
    /// server at its own network. Refused rather than dropped, so a caller
    /// that sent one learns it did not land.
    fn admit_transport(&self, profile: bool, proxy: bool) -> Result<()> {
        if self.scope.is_instance() || (!profile && !proxy) {
            return Ok(());
        }
        Err(AppError::forbidden(
            "`connectionProfileId` and `proxy` are set by an instance administrator",
        ))
    }

    /// The row as it is, then the row as the patch would leave it.
    async fn admit_patch(&self, id: &str, patch: &CredentialPatch) -> Result<()> {
        self.admit(id).await?;
        if self.scope.is_instance() {
            return Ok(());
        }
        self.admit_transport(patch.connection_profile_id.is_some(), patch.proxy.is_some())?;
        if patch.user_id.is_none() && patch.team_id.is_none() && patch.organization_id.is_none() {
            return Ok(());
        }
        // A patch that touches ownership is rare and always deliberate, so the
        // row is read rather than taken from the snapshot: the merge has to be
        // against the row the write will actually land on.
        let row = self
            .gproxy
            .store()
            .credentials()
            .get_many(&[id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| AppError::not_found("credential", id))?;
        let merged = |patched: &Option<Option<String>>, current: &Option<String>| {
            patched.clone().unwrap_or_else(|| current.clone())
        };
        self.admit_owner(
            merged(&patch.user_id, &row.user_id).as_deref(),
            merged(&patch.team_id, &row.team_id).as_deref(),
            merged(&patch.organization_id, &row.organization_id).as_deref(),
        )
    }
}

// ---------------------------------------------------------------- quotas --

pub struct ScopedQuotas<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    scope: &'a AdminScope,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ScopedQuotas<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<QuotaDto>> {
        let bounds = query.bounds();
        // A credential's limits are asked for by the credential's id, which
        // `narrow` cannot place in a scope; admit the credential instead.
        if !self.scope.is_instance()
            && query.owner_kind.as_deref().map(str::trim) == Some("credential")
        {
            let Some(id) = query
                .owner_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                return Ok(empty(bounds));
            };
            if !self
                .scope
                .admits_credential(self.gproxy, self.data, id)
                .await?
            {
                return Ok(empty(bounds));
            }
            return Ok(self.manage().list(query).await?);
        }
        match self.scope.narrow(query, self.data) {
            ScopedQuery::Nothing => Ok(empty(bounds)),
            ScopedQuery::Run(query) => Ok(self.manage().list(*query).await?),
        }
    }

    pub async fn get(&self, id: &str) -> Result<QuotaDto> {
        self.admit(id).await?;
        Ok(self.manage().get(id).await?)
    }

    pub async fn create(&self, write: QuotaWrite) -> Result<QuotaDto> {
        self.admit_owner(&write.owner_kind, &write.owner_id).await?;
        Ok(self.manage().create(write).await?)
    }

    pub async fn update(&self, id: &str, patch: QuotaPatch) -> Result<QuotaDto> {
        self.admit_patch(id, &patch).await?;
        Ok(self.manage().update(id, patch).await?)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.admit(id).await?;
        Ok(self.manage().delete(id).await?)
    }

    pub async fn batch(
        &self,
        items: Vec<BatchItem<QuotaWrite, QuotaPatch>>,
    ) -> Result<Vec<Option<QuotaDto>>> {
        for item in &items {
            match item {
                BatchItem::Create(write) => {
                    self.admit_owner(&write.owner_kind, &write.owner_id).await?
                }
                BatchItem::Update(step) => self.admit_patch(&step.id, &step.patch).await?,
                BatchItem::Delete(id) => self.admit(id).await?,
            }
        }
        Ok(self.manage().batch(items).await?)
    }

    /// The current window of every budget of `owners`.
    ///
    /// Every owner the caller named is admitted before the read, rather than
    /// the answer being filtered afterwards: a status list is how a console
    /// polls a budget bar, and an owner that came back empty because it was
    /// filtered is indistinguishable from one with no budget — which would
    /// make this an owner-existence oracle.
    pub async fn budget_status(&self, owners: &[BudgetOwner]) -> Result<Vec<BudgetStatusDto>> {
        for owner in owners {
            self.admit_owner(&owner.kind, &owner.id).await?;
        }
        Ok(self.manage().budget_status(owners).await?)
    }

    pub async fn reset_budget(&self, id: &str) -> Result<BudgetStatusDto> {
        self.admit(id).await?;
        Ok(self.manage().reset_budget(id).await?)
    }

    /// The other half of the same table. An operator limit is owned by a
    /// `credential` or a `provider`; outside the instance scope only the
    /// former is ever reachable, and only when the credential is.
    pub async fn reset_limit(&self, id: &str) -> Result<Vec<String>> {
        self.admit(id).await?;
        Ok(self.manage().reset_limit(id).await?)
    }

    fn manage(&self) -> gproxy_sdk::manage::Quotas<'_, C> {
        self.gproxy.manage().quotas()
    }

    async fn admit(&self, id: &str) -> Result<()> {
        self.scope.admit_quota(self.gproxy, self.data, id).await?;
        Ok(())
    }

    /// A credential limit names a credential the caller may not be able to
    /// see, so it answers `NotFound` like every other read of one; the other
    /// kinds name an owner the caller typed and answer `Forbidden`.
    async fn admit_owner(&self, owner_kind: &str, owner_id: &str) -> Result<()> {
        if owner_kind == "credential" {
            return self
                .scope
                .admit_credential(self.gproxy, self.data, owner_id)
                .await;
        }
        self.scope
            .admit_write(ScopeOwner::from_pair(owner_kind, owner_id), self.data)
    }

    async fn admit_patch(&self, id: &str, patch: &QuotaPatch) -> Result<()> {
        let row = self.scope.admit_quota(self.gproxy, self.data, id).await?;
        if self.scope.is_instance() {
            return Ok(());
        }
        if patch.owner_kind.is_none() && patch.owner_id.is_none() {
            return Ok(());
        }
        self.admit_owner(
            patch.owner_kind.as_deref().unwrap_or(&row.owner_kind),
            patch.owner_id.as_deref().unwrap_or(&row.owner_id),
        )
        .await
    }
}
