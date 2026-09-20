//! Configuration writes: the families a management UI drives.
//!
//! Every write is one `Store::commit_revision` batch, so the rows and the
//! `config_revision` bump land together — a write that landed without a bump
//! would be invisible to peers, and a bump without a write would make them
//! reload for nothing. After the batch this instance reloads, and only then is
//! `Invalidation::ConfigurationChanged` published for the peers: a notification
//! must never arrive before the writer itself can serve what it announced.
//!
//! The publication is best effort. A cache that refuses it costs the
//! deployment one poll interval, not correctness, so it is a warning and never
//! an error.
//!
//! Which reload follows is the only thing the caller chooses, through the
//! scopes it names. Everything about a credential except its secret, expiry
//! and lifecycle status is frozen into `CredentialData` at assembly, so only a
//! write limited to those may claim [`Scope::CredentialState`] and take the
//! cheap `reload_credentials` path. Everything else rebuilds the snapshot.

mod credentials;
mod crud;
mod endpoints;
mod models;
mod pricing;
mod profiles;
mod providers;
mod quotas;
mod rewrite;
mod routing;
mod settings;

pub use credentials::Credentials;
pub use endpoints::Endpoints;
pub use models::{Models, ProviderModels};
pub use pricing::{PriceRates, PriceRules, PriceTiers, Pricing};
pub use profiles::ConnectionProfiles;
pub use providers::Providers;
pub use quotas::Quotas;
pub use rewrite::{ProviderRuleSets, Rewrite, RewriteRules, RuleSets};
pub use routing::{ExposedModels, RouteMembers, Routes};
pub use settings::SettingsManage;

use std::sync::Arc;

use gproxy_core::{Invalidation, keys};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{Store, StoreError};

use crate::{SdkError, SdkResult, handle::Inner};

/// Which configuration families a write touched. The name travels to the peers
/// inside the notification; the variant decides what this instance reloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Providers,
    /// A credential write that may have changed something the snapshot froze:
    /// the label, the auth kind, the metadata, the connection profile, the
    /// owner, or the row's very existence. Forces a full reload.
    Credentials(Vec<String>),
    /// A credential write limited to the secret, its expiry and the lifecycle
    /// status — exactly what `Core::reload_credentials` re-reads. Never use it
    /// for anything else: the change would land in the database and stay
    /// invisible to this instance until the next unrelated reload.
    CredentialState(Vec<String>),
    Routing,
    Profiles,
    Settings,
    Rewrite,
    Endpoints,
    Quotas,
    Pricing,
    Models,
}

impl Scope {
    /// The string a peer sees in `ConfigurationChanged::scopes`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Providers => "providers",
            Self::Credentials(_) => "credentials",
            Self::CredentialState(_) => "credential_state",
            Self::Routing => "routing",
            Self::Profiles => "profiles",
            Self::Settings => "settings",
            Self::Rewrite => "rewrite",
            Self::Endpoints => "endpoints",
            Self::Quotas => "quotas",
            Self::Pricing => "pricing",
            Self::Models => "models",
        }
    }
}

/// The write side of a handle, one accessor per configuration family.
pub struct Manage<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

impl<'a, C> Manage<'a, C> {
    pub(crate) fn new(inner: &'a Arc<Inner<C>>) -> Self {
        Self { inner }
    }

    fn writer(&self) -> Writer<'a, C> {
        Writer { inner: self.inner }
    }

    pub fn providers(&self) -> Providers<'a, C> {
        Providers::new(self.writer())
    }
    pub fn credentials(&self) -> Credentials<'a, C> {
        Credentials::new(self.writer())
    }
    pub fn models(&self) -> Models<'a, C> {
        Models::new(self.writer())
    }
    pub fn provider_models(&self) -> ProviderModels<'a, C> {
        ProviderModels::new(self.writer())
    }
    pub fn routes(&self) -> Routes<'a, C> {
        Routes::new(self.writer())
    }
    pub fn route_members(&self) -> RouteMembers<'a, C> {
        RouteMembers::new(self.writer())
    }
    pub fn exposed_models(&self) -> ExposedModels<'a, C> {
        ExposedModels::new(self.writer())
    }
    pub fn connection_profiles(&self) -> ConnectionProfiles<'a, C> {
        ConnectionProfiles::new(self.writer())
    }
    pub fn settings(&self) -> SettingsManage<'a, C> {
        SettingsManage::new(self.writer())
    }
    pub fn rewrite(&self) -> Rewrite<'a, C> {
        Rewrite::new(self.writer())
    }
    pub fn endpoints(&self) -> Endpoints<'a, C> {
        Endpoints::new(self.writer())
    }
    pub fn quotas(&self) -> Quotas<'a, C> {
        Quotas::new(self.writer())
    }
    pub fn pricing(&self) -> Pricing<'a, C> {
        Pricing::new(self.writer())
    }
}

/// The one write primitive every family goes through.
pub(crate) struct Writer<'a, C> {
    inner: &'a Arc<Inner<C>>,
}

// Manual, because a derive would demand `C: Copy` for a field that is a
// reference either way.
impl<C> Clone for Writer<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Copy for Writer<'_, C> {}

impl<'a, C> Writer<'a, C> {
    pub(crate) fn inner(&self) -> &'a Arc<Inner<C>> {
        self.inner
    }
    pub(crate) fn store(&self) -> &'a Store<C> {
        &self.inner.store
    }
    pub(crate) fn core(&self) -> &'a gproxy_core::Core<C> {
        &self.inner.core
    }
}

impl<C: BatchConnectionTrait> Writer<'_, C> {
    pub(crate) fn backend(&self) -> sea_orm::DbBackend {
        self.store().connection().get_database_backend()
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Writer<'_, C> {
    /// Run `statements` and the revision bump as one transaction, bring this
    /// instance up to the result, then tell the peers. Returns the durable
    /// revision the write produced.
    pub(crate) async fn commit(
        &self,
        statements: Vec<BatchStatement>,
        scopes: &[Scope],
    ) -> SdkResult<i64> {
        let commit = self
            .store()
            .commit_revision(statements)
            .await
            .map_err(classify)?;
        self.after_write(commit.revision, scopes).await?;
        Ok(commit.revision)
    }

    /// The same, keeping the caller statements' results: a family that reads
    /// its row back inside the transaction needs them.
    pub(crate) async fn commit_results(
        &self,
        statements: Vec<BatchStatement>,
        scopes: &[Scope],
    ) -> SdkResult<(i64, Vec<gproxy_seaorm::BatchResult>)> {
        let commit = self
            .store()
            .commit_revision(statements)
            .await
            .map_err(classify)?;
        self.after_write(commit.revision, scopes).await?;
        Ok((commit.revision, commit.results))
    }

    /// Reload, then notify — never the other way round. A peer that reloaded
    /// on our notification must not find this instance still serving the old
    /// revision it just announced.
    async fn after_write(&self, revision: i64, scopes: &[Scope]) -> SdkResult<()> {
        match state_only(scopes) {
            Some(ids) if !ids.is_empty() => {
                self.inner.reload_credentials(&ids).await?;
            }
            // An empty CredentialState list has nothing to re-read, and no
            // scope at all is a caller that did not classify its write: both
            // take the safe path.
            _ => {
                self.inner.reload().await?;
            }
        }
        let names: Vec<String> = scopes.iter().map(|scope| scope.name().to_owned()).collect();
        let payload = Invalidation::ConfigurationChanged {
            revision: gproxy_core::ConfigRevision(u64::try_from(revision).unwrap_or(0)),
            scopes: names,
        };
        // Losing this costs the deployment one poll interval, nothing more.
        match serde_json::to_vec(&payload) {
            Ok(bytes) => {
                if let Err(error) = self
                    .inner
                    .cache
                    .publish(keys::INVALIDATION_TOPIC, bytes)
                    .await
                {
                    tracing::warn!(%error, revision, "invalidation not published; peers converge on the revision poll");
                }
            }
            Err(error) => tracing::warn!(%error, "invalidation payload could not be encoded"),
        }
        Ok(())
    }
}

/// The credential ids of a write that touches nothing but credential state,
/// or None when any other scope is present.
fn state_only(scopes: &[Scope]) -> Option<Vec<String>> {
    if scopes.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for scope in scopes {
        match scope {
            Scope::CredentialState(scoped) => ids.extend(scoped.iter().cloned()),
            _ => return None,
        }
    }
    Some(ids)
}

/// A constraint the database enforced rather than this layer. Pre-checks catch
/// the common cases, but two writers racing for the same name only collide
/// here, and a caller told "500" could not tell the difference.
fn classify(error: StoreError) -> SdkError {
    let text = error.to_string();
    let lowered = text.to_ascii_lowercase();
    const UNIQUE: [&str; 4] = [
        "unique constraint",
        "duplicate key",
        "duplicate entry",
        "unique failed",
    ];
    if UNIQUE.iter().any(|needle| lowered.contains(needle)) {
        return SdkError::conflict(text);
    }
    if lowered.contains("foreign key") {
        return SdkError::invalid(text);
    }
    SdkError::Store(error)
}
