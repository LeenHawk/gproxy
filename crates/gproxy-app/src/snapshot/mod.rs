//! The identity snapshot: the rows of one configuration revision, compiled
//! into the indexes admission needs, published as one immutable value.
//!
//! [`AppData`] is to this crate what `CoreData` is to `gproxy-core`, and the
//! two are deliberately assembled from the same read: `Store::load_all_data`
//! returns control, routing and identity rows in one batch, so the engine and
//! the product layer are never a revision apart in the middle of a request.
//! A request takes one [`AppSnapshot::load`] and keeps that `Arc` for its
//! whole life; a reload that lands mid-request changes nothing it has already
//! decided.
//!
//! Assembly never fails on a bad row. A single malformed permission or an
//! undecodable key digest must not take down an instance that is otherwise
//! serving traffic, so such rows are dropped, counted and logged, and the
//! snapshot is published without them.

mod credentials;
mod glob;
mod keys;
mod membership;
mod oauth_policy;
mod permission;

pub use credentials::{CredentialOwnership, Owner};
pub use keys::{ApiKeyIdentity, ApiKeyIndex, decode_key_hash, encode_key_hash};
pub use membership::MembershipIndex;
pub use oauth_policy::ClientAllowlist;
pub use permission::{Decision, PermissionSet, Subject};

use crate::AppError;
use arc_swap::ArcSwap;
use gproxy_store::{
    IdentityData,
    entity::{
        identity::{organization, team, user},
        limits::rate_limit,
        oauth,
        subscription::{plan, plan_limit, pool, pool_member, user_subscription},
        upstream::credential,
    },
};
use std::{collections::HashMap, sync::Arc};

/// Everything the product layer knows about callers at one revision.
///
/// Immutable once built. The indexes hold what a decision needs; the maps hold
/// the rows a decision has to report or re-check, keyed by id so neither
/// needs a scan.
///
/// No derived `Debug` or `Serialize`, for the same reason `CoreData` has
/// none: the rows here carry password hashes and retained key material, and a
/// tracing field or an error body that formats the whole snapshot would put
/// them in a log. The manual `Debug` reports sizes only.
pub struct AppData {
    /// `settings.config_revision` this was assembled from.
    pub revision: i64,
    pub keys: ApiKeyIndex,
    pub memberships: MembershipIndex,
    pub credential_ownership: CredentialOwnership,
    pub permissions: PermissionSet,
    pub oauth_client_allowlist: ClientAllowlist,
    pub users: HashMap<String, user::Model>,
    pub organizations: HashMap<String, organization::Model>,
    pub teams: HashMap<String, team::Model>,
    /// Configured limits; the live counters are in the cache. Admission
    /// selects the applicable ones per request.
    pub rate_limits: Vec<rate_limit::Model>,
    pub subscriptions: HashMap<String, user_subscription::Model>,
    pub plans: HashMap<String, plan::Model>,
    /// plan_id → its allowance rows.
    pub plan_limits: HashMap<String, Vec<plan_limit::Model>>,
    pub pools: HashMap<String, pool::Model>,
    /// pool_id → the credentials that back it.
    pub pool_members: HashMap<String, Vec<pool_member::Model>>,
    pub oauth_clients: HashMap<String, oauth::client::Model>,
}

impl AppData {
    /// Compile one revision.
    ///
    /// `credentials` comes from `ControlData` rather than `IdentityData`
    /// because a credential is an engine object that happens to carry an owner;
    /// both are taken as arguments so a caller passes the two halves of a
    /// single `load_all_data()` and cannot accidentally mix revisions.
    pub fn assemble(
        revision: i64,
        identity: &IdentityData,
        credentials: &[credential::Model],
    ) -> Result<Self, AppError> {
        Self::assemble_at(revision, identity, credentials, crate::now_ms())
    }

    /// [`AppData::assemble`] against a stated clock. Only key expiry depends
    /// on it; tests use this to pin the boundary.
    pub fn assemble_at(
        revision: i64,
        identity: &IdentityData,
        credentials: &[credential::Model],
        now_ms: i64,
    ) -> Result<Self, AppError> {
        let users: HashMap<String, user::Model> = by_id(&identity.users, |row| &row.id);
        let mut plan_limits: HashMap<String, Vec<plan_limit::Model>> = HashMap::new();
        for row in &identity.plan_limits {
            plan_limits
                .entry(row.plan_id.clone())
                .or_default()
                .push(row.clone());
        }
        let mut pool_members: HashMap<String, Vec<pool_member::Model>> = HashMap::new();
        for row in &identity.pool_members {
            pool_members
                .entry(row.pool_id.clone())
                .or_default()
                .push(row.clone());
        }
        Ok(Self {
            revision,
            keys: ApiKeyIndex::build(&identity.api_keys, &users, now_ms),
            memberships: MembershipIndex::build(
                &identity.organization_members,
                &identity.team_members,
                &identity.teams,
            ),
            credential_ownership: CredentialOwnership::build(credentials),
            permissions: PermissionSet::build(&identity.permissions),
            oauth_client_allowlist: ClientAllowlist::build(
                &identity.users,
                &identity.organizations,
                &identity.teams,
            ),
            users,
            organizations: by_id(&identity.organizations, |row| &row.id),
            teams: by_id(&identity.teams, |row| &row.id),
            rate_limits: identity.rate_limits.clone(),
            subscriptions: by_id(&identity.subscriptions, |row| &row.id),
            plans: by_id(&identity.plans, |row| &row.id),
            plan_limits,
            pools: by_id(&identity.pools, |row| &row.id),
            pool_members,
            oauth_clients: by_id(&identity.oauth_clients, |row| &row.id),
        })
    }

    /// An instance that has loaded nothing yet. Its revision is below every
    /// durable one, including the `0` a freshly created database starts at, so
    /// the first load always replaces it.
    pub fn empty() -> Self {
        Self {
            revision: i64::MIN,
            keys: ApiKeyIndex::default(),
            memberships: MembershipIndex::default(),
            credential_ownership: CredentialOwnership::default(),
            permissions: PermissionSet::default(),
            oauth_client_allowlist: ClientAllowlist::default(),
            users: HashMap::new(),
            organizations: HashMap::new(),
            teams: HashMap::new(),
            rate_limits: Vec::new(),
            subscriptions: HashMap::new(),
            plans: HashMap::new(),
            plan_limits: HashMap::new(),
            pools: HashMap::new(),
            pool_members: HashMap::new(),
            oauth_clients: HashMap::new(),
        }
    }

    /// The effective organization of a key binding: the key's own, or the
    /// parent of the team it is bound to. This is the value credential
    /// visibility and the budget chain are evaluated against.
    pub fn effective_organization<'a>(
        &'a self,
        organization_id: Option<&'a str>,
        team_id: Option<&'a str>,
    ) -> Option<&'a str> {
        organization_id.or_else(|| team_id.and_then(|team| self.memberships.parent_of(team)))
    }
}

impl std::fmt::Debug for AppData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppData")
            .field("revision", &self.revision)
            .field("keys", &self.keys.len())
            .field("users", &self.users.len())
            .field("organizations", &self.organizations.len())
            .field("teams", &self.teams.len())
            .field("permissions", &self.permissions.len())
            .field("credentials", &self.credential_ownership.len())
            .field("rate_limits", &self.rate_limits.len())
            .field("oauth_clients", &self.oauth_clients.len())
            .finish_non_exhaustive()
    }
}

fn by_id<M: Clone>(rows: &[M], id: impl Fn(&M) -> &String) -> HashMap<String, M> {
    rows.iter()
        .map(|row| (id(row).clone(), row.clone()))
        .collect()
}

/// The published snapshot. Reads are wait-free; a reload swaps one pointer.
pub struct AppSnapshot(ArcSwap<AppData>);

impl std::fmt::Debug for AppSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AppSnapshot").field(&*self.0.load()).finish()
    }
}

impl AppSnapshot {
    pub fn new(data: Arc<AppData>) -> Self {
        Self(ArcSwap::new(data))
    }

    /// The snapshot in force. Hold the returned `Arc` for the whole request:
    /// two loads within one request could straddle a reload and disagree.
    pub fn load(&self) -> Arc<AppData> {
        self.0.load_full()
    }

    pub fn revision(&self) -> i64 {
        self.0.load().revision
    }

    /// Publish `next` unless something at least as new is already active, and
    /// answer whether it was published.
    ///
    /// Same contract as `Core::publish_snapshot`: reloads race — a poll and a
    /// notification can both fire for the same write, and a slow load started
    /// before a fast one can finish after it — and a snapshot going backwards
    /// would resurrect deleted keys and revoked grants until the next reload.
    /// Monotonicity by revision is what makes an out-of-order reload harmless
    /// rather than a security regression.
    pub fn publish_if_newer(&self, next: Arc<AppData>) -> bool {
        let previous = self.0.rcu(|current| {
            if next.revision > current.revision {
                next.clone()
            } else {
                current.clone()
            }
        });
        next.revision > previous.revision
    }
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self::new(Arc::new(AppData::empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(revision: i64) -> Arc<AppData> {
        let mut data = AppData::empty();
        data.revision = revision;
        Arc::new(data)
    }

    #[test]
    fn an_empty_snapshot_is_older_than_a_fresh_database() {
        let snapshot = AppSnapshot::default();
        assert_eq!(snapshot.revision(), i64::MIN);
        assert!(snapshot.publish_if_newer(data(0)));
        assert_eq!(snapshot.revision(), 0);
    }

    #[test]
    fn a_newer_revision_replaces_the_active_one() {
        let snapshot = AppSnapshot::new(data(4));
        assert!(snapshot.publish_if_newer(data(5)));
        assert_eq!(snapshot.load().revision, 5);
    }

    #[test]
    fn an_older_or_equal_revision_is_ignored() {
        let snapshot = AppSnapshot::new(data(5));
        assert!(!snapshot.publish_if_newer(data(4)));
        assert_eq!(snapshot.revision(), 5);
        assert!(!snapshot.publish_if_newer(data(5)));
        assert_eq!(snapshot.revision(), 5);
    }

    #[test]
    fn an_empty_revision_assembles_to_empty_indexes() {
        let data = AppData::assemble(3, &IdentityData::default(), &[]).unwrap();
        assert_eq!(data.revision, 3);
        assert!(data.keys.is_empty());
        assert!(data.permissions.is_empty());
        assert!(data.credential_ownership.is_empty());
        assert!(data.users.is_empty());
    }
}
