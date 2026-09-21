//! The identity write surface: users, gateway keys, organizations, teams,
//! memberships, permissions, rate limits, subscriptions, pools, plans, OAuth
//! clients, sessions and the audit trail.
//!
//! Every family is `list / get / create / update / delete` over one table,
//! plus whatever else that family genuinely needs — a password, a key
//! rotation, a retirement. The five come from one generic `crud::Shape`, so
//! one place decides that a create reads its row back inside its own
//! transaction, that a missing id is `NotFound` rather than a silent no-op,
//! and that a batch is one revision commit however many rows it names.
//!
//! # The write contract, and how it differs from the sdk's
//!
//! A configuration write here is one [`Store::commit_revision`] batch: the
//! caller's statements and the `settings.config_revision` bump land together
//! or not at all. A write that landed without a bump would be invisible to
//! peers; a bump without a write would make them reload for nothing.
//!
//! After the batch, the writer publishes `Invalidation::ConfigurationChanged`
//! on `gproxy_core::keys::INVALIDATION_TOPIC` with this crate's own scope
//! names. **It does not reload anything itself**, and that is the one place it
//! parts company with `gproxy-sdk`'s `manage::Writer`:
//!
//! - the sdk owns the snapshot it writes into, so it reloads and only then
//!   notifies — a peer must never find the writer still serving the revision
//!   it just announced;
//! - identity rows are not in `CoreData` at all. What they feed is
//!   [`AppData`], which is owned by the host: it holds the `AppSnapshot`, it
//!   decides when a request's view advances, and it rebuilds through
//!   `App::refresh()`. A writer that reloaded on its own would publish a
//!   second, competing snapshot and lose the monotonicity `AppSnapshot`
//!   exists to guarantee.
//!
//! So the contract is: **the operation commits and notifies; the host
//! refreshes.** A caller is never shown a stale answer for its own write
//! regardless, because every create and update reads its row back inside the
//! same transaction rather than from the snapshot.
//!
//! Publication is best effort. A cache that refuses it costs the deployment
//! one poll interval, not correctness, so it is a warning and never an error.
//!
//! # What is not a configuration write
//!
//! [`Sessions`] and [`crate::audit`] write rows that `AppData` does not
//! contain: a session is resolved by a database read on every request, and the
//! audit trail is history that nothing reads to serve one. Neither bumps the
//! revision and neither notifies — doing so would make every login and every
//! audited operation invalidate every peer's snapshot for no observable
//! change.
//!
//! # The other surface
//!
//! Everything above is the **operator's**: a family takes an id and acts on
//! whatever row it names, and who may call it is the host's decision.
//! [`Operations::portal`] hands out the **end user's** surface, which is
//! shaped the opposite way — it is built from a [`Caller`](crate::Caller), no
//! method on it takes a user id, and it delegates the writes back to the
//! families here so no rule is validated twice. See [`portal`].

mod api_keys;
mod crud;
pub mod issuer;
mod members;
mod oauth_clients;
mod organizations;
mod permissions;
mod plans;
mod pools;
pub mod portal;
mod rate_limits;
mod scoped;
mod sessions;
mod subscriptions;
mod users;

pub use api_keys::ApiKeys;
pub use issuer::{Issuer, IssuerError, IssuerErrorCode, IssuerOrigin};
pub use members::{OrganizationMembers, TeamMembers};
pub use oauth_clients::OAuthClients;
pub use organizations::{Organizations, Teams};
pub use permissions::Permissions;
pub use plans::{PlanLimits, Plans};
pub use pools::{PoolMembers, Pools};
pub use portal::{
    MAX_PORTAL_MODELS, MAX_RECENT_REQUESTS, Portal, PortalKeys, PortalOAuthSessions, PortalPassword,
};
pub use rate_limits::RateLimits;
pub use scoped::{ScopedCredentials, ScopedManage, ScopedQuotas};
pub use sessions::Sessions;
pub use subscriptions::Subscriptions;
pub use users::Users;

pub(crate) use crud::random_id;

use gproxy_core::{Invalidation, keys};
use gproxy_sdk::Gproxy;
use gproxy_seaorm::{BatchConnectionTrait, BatchResult, BatchStatement};
use gproxy_store::{Store, StoreError};

use crate::{AppConfig, AppData, AppError, Result, audit::Audit};

/// Which identity families a write touched.
///
/// The name travels to the peers inside `ConfigurationChanged::scopes`, where
/// core treats it as an opaque host string. A subscriber that only rebuilds
/// `AppData` can ignore every sdk scope, and vice versa; a subscriber that
/// does not recognise a name reloads everything, which is always correct and
/// merely slower.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Users, organizations, teams and memberships: the actors and the scopes
    /// they act in.
    Identity,
    /// Permission rules.
    Permissions,
    /// Gateway API keys, including their bindings.
    Keys,
    /// Configured rate limits. The counters live in the cache and are not
    /// affected by a write.
    RateLimits,
    /// Pools, plans, plan limits and issued subscriptions.
    Subscriptions,
    /// The registered OAuth client list.
    OAuthClients,
}

impl Scope {
    /// The string a peer sees.
    pub fn name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Permissions => "permissions",
            Self::Keys => "keys",
            Self::RateLimits => "rate_limits",
            Self::Subscriptions => "subscriptions",
            Self::OAuthClients => "oauth_clients",
        }
    }
}

/// The identity operation families, over one handle, one snapshot and one
/// configuration.
///
/// Borrowed rather than owned, for the same reason [`crate::Authenticator`] is:
/// a request already holds the `Arc<AppData>` it decided under, and the whole
/// point of a snapshot is that one value answers every question in that
/// request.
///
/// This type performs **no authorization**. Who may call which family is the
/// host's middleware decision, made from the `Caller` before the operation
/// runs; an `Operations` in hand means that decision has already been taken.
pub struct Operations<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    config: &'a AppConfig,
}

impl<'a, C> Operations<'a, C> {
    pub fn new(gproxy: &'a Gproxy<C>, data: &'a AppData, config: &'a AppConfig) -> Self {
        Self {
            gproxy,
            data,
            config,
        }
    }

    /// The identity snapshot these operations validate against. It can
    /// legitimately be one revision behind the database — a row written by the
    /// operation before this one is not in it — so it is used as a fast path
    /// and never as the last word: every check that can refuse a write falls
    /// back to a read.
    pub fn data(&self) -> &'a AppData {
        self.data
    }

    pub fn config(&self) -> &'a AppConfig {
        self.config
    }

    pub fn store(&self) -> &'a Store<C> {
        self.gproxy.store()
    }

    fn writer(&self) -> Writer<'a, C> {
        Writer {
            gproxy: self.gproxy,
            data: self.data,
        }
    }

    pub fn users(&self) -> Users<'a, C> {
        Users::new(self.writer())
    }
    pub fn api_keys(&self) -> ApiKeys<'a, C> {
        ApiKeys::new(self.writer())
    }
    pub fn organizations(&self) -> Organizations<'a, C> {
        Organizations::new(self.writer())
    }
    pub fn teams(&self) -> Teams<'a, C> {
        Teams::new(self.writer())
    }
    /// Organization membership. Team membership is [`Operations::team_members`];
    /// the two are separate tables with separate composite keys.
    pub fn members(&self) -> OrganizationMembers<'a, C> {
        OrganizationMembers::new(self.writer())
    }
    pub fn team_members(&self) -> TeamMembers<'a, C> {
        TeamMembers::new(self.writer())
    }
    pub fn permissions(&self) -> Permissions<'a, C> {
        Permissions::new(self.writer())
    }
    pub fn rate_limits(&self) -> RateLimits<'a, C> {
        RateLimits::new(self.writer())
    }
    pub fn subscriptions(&self) -> Subscriptions<'a, C> {
        Subscriptions::new(self.writer())
    }
    pub fn pools(&self) -> Pools<'a, C> {
        Pools::new(self.writer())
    }
    pub fn pool_members(&self) -> PoolMembers<'a, C> {
        PoolMembers::new(self.writer())
    }
    pub fn plans(&self) -> Plans<'a, C> {
        Plans::new(self.writer())
    }
    pub fn plan_limits(&self) -> PlanLimits<'a, C> {
        PlanLimits::new(self.writer())
    }
    pub fn oauth_clients(&self) -> OAuthClients<'a, C> {
        OAuthClients::new(self.writer())
    }
    /// The OAuth authorization server this instance runs **for downstream
    /// clients** — authorize, token, device, revoke and the RFC 8414
    /// discovery document. Not a configuration surface: see
    /// [`issuer`] for why none of it moves the
    /// revision.
    pub fn issuer(&self) -> Issuer<'a, C> {
        Issuer::new(self.gproxy, self.data, self.config)
    }
    /// Console and portal sessions. Not configuration: see the module note.
    pub fn sessions(&self) -> Sessions<'a, C> {
        Sessions::new(self.writer())
    }
    /// The audit trail. Not configuration either, and written outside the
    /// revision batch of the operation it records.
    pub fn audit(&self) -> Audit<'a, C> {
        Audit::new(self.gproxy.store())
    }
}

/// The one write primitive every family goes through.
pub(crate) struct Writer<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
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
    pub(crate) fn store(&self) -> &'a Store<C> {
        self.gproxy.store()
    }
    pub(crate) fn gproxy(&self) -> &'a Gproxy<C> {
        self.gproxy
    }
    pub(crate) fn data(&self) -> &'a AppData {
        self.data
    }
}

impl<C: BatchConnectionTrait> Writer<'_, C> {
    pub(crate) fn backend(&self) -> sea_orm::DbBackend {
        self.store().connection().get_database_backend()
    }
}

impl<C: BatchConnectionTrait> Writer<'_, C> {
    /// Run `statements` and the revision bump as one transaction, then tell
    /// the peers. Returns the durable revision the write produced.
    pub(crate) async fn commit(
        &self,
        statements: Vec<BatchStatement>,
        scopes: &[Scope],
    ) -> Result<i64> {
        Ok(self.commit_results(statements, scopes).await?.0)
    }

    /// The same, keeping the caller statements' results: a family that reads
    /// its row back inside the transaction needs them.
    pub(crate) async fn commit_results(
        &self,
        statements: Vec<BatchStatement>,
        scopes: &[Scope],
    ) -> Result<(i64, Vec<BatchResult>)> {
        let commit = self
            .store()
            .commit_revision(statements)
            .await
            .map_err(classify)?;
        self.notify(commit.revision, scopes).await;
        Ok((commit.revision, commit.results))
    }

    /// Tell the peers a revision landed and which families it touched.
    ///
    /// Nothing is reloaded here; rebuilding [`AppData`] belongs to the host's
    /// `App::refresh()`, which owns the snapshot and its monotonic
    /// publication. Losing this notification costs the deployment one poll
    /// interval, so a failure is a warning.
    async fn notify(&self, revision: i64, scopes: &[Scope]) {
        let payload = Invalidation::ConfigurationChanged {
            revision: gproxy_core::ConfigRevision(u64::try_from(revision).unwrap_or(0)),
            scopes: scopes.iter().map(|scope| scope.name().to_owned()).collect(),
        };
        match serde_json::to_vec(&payload) {
            Ok(bytes) => {
                if let Err(error) = self
                    .gproxy
                    .cache()
                    .publish(keys::INVALIDATION_TOPIC, bytes)
                    .await
                {
                    tracing::warn!(
                        %error,
                        revision,
                        "invalidation not published; peers converge on the revision poll"
                    );
                }
            }
            Err(error) => tracing::warn!(%error, "invalidation payload could not be encoded"),
        }
    }
}

/// A constraint the database enforced rather than this layer. Pre-checks catch
/// the common cases, but two writers racing for the same name only collide
/// here, and a caller told "500" could not tell the difference.
fn classify(error: StoreError) -> AppError {
    let text = error.to_string();
    let lowered = text.to_ascii_lowercase();
    const UNIQUE: [&str; 4] = [
        "unique constraint",
        "duplicate key",
        "duplicate entry",
        "unique failed",
    ];
    if UNIQUE.iter().any(|needle| lowered.contains(needle)) {
        return AppError::Conflict(text);
    }
    if lowered.contains("foreign key") {
        return AppError::Invalid(text);
    }
    AppError::Store(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_names_are_the_ones_peers_subscribe_to() {
        // These strings are a contract with every other instance: a rename is
        // a deployment where half the fleet stops recognising the other half.
        assert_eq!(Scope::Identity.name(), "identity");
        assert_eq!(Scope::Permissions.name(), "permissions");
        assert_eq!(Scope::Keys.name(), "keys");
        assert_eq!(Scope::RateLimits.name(), "rate_limits");
        assert_eq!(Scope::Subscriptions.name(), "subscriptions");
        assert_eq!(Scope::OAuthClients.name(), "oauth_clients");
    }

    #[test]
    fn a_constraint_violation_is_a_conflict_not_a_server_error() {
        let error = classify(StoreError::Invalid(
            "UNIQUE constraint failed: users.name".into(),
        ));
        assert_eq!(error.status_code(), 409);
        let error = classify(StoreError::Invalid("FOREIGN KEY constraint failed".into()));
        assert_eq!(error.status_code(), 400);
        let error = classify(StoreError::UnexpectedResult);
        assert_eq!(error.status_code(), 500);
    }
}
