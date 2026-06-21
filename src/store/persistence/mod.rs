//! Durable storage abstraction.
//!
//! Native impls: `file` (local disk, single-instance) and `db` (SeaORM, multi-instance).
//! Edge (wasm32) impl: `libsql` (libSQL/Turso over Hrana HTTP).
//! Domain code calls only trait methods.

#[cfg(all(not(target_arch = "wasm32"), feature = "persist-db"))]
pub mod db;
#[cfg(all(not(target_arch = "wasm32"), feature = "persist-file"))]
pub mod file;

#[cfg(all(target_arch = "wasm32", feature = "persist-libsql"))]
pub mod libsql;

pub mod metrics;
#[cfg(any(
    all(not(target_arch = "wasm32"), feature = "persist-db"),
    all(not(target_arch = "wasm32"), feature = "persist-file"),
    all(target_arch = "wasm32", feature = "persist-libsql")
))]
pub mod migrations;
pub mod records;

/// A unique-constraint violation from an upsert (duplicate name/alias/digest,
/// or a composite-key collision). Backends return this carried inside
/// `anyhow::Error`; the admin HTTP layer downcasts it to map to 409 Conflict
/// instead of a generic 500. Kept in the (wasm-agnostic) store layer so the
/// persistence backends never depend on the HTTP error type.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ConflictError(pub String);

impl ConflictError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

/// Filter + cursor for the usage explorer (B4). All filters optional; `before_id`
/// is the keyset cursor (rows have `id` DESC, so "next page" = id < before_id).
#[derive(Debug, Default, Clone)]
pub struct UsageQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub provider_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: u64,
}

#[cfg(all(not(target_arch = "wasm32"), feature = "persist-db"))]
pub use db::DbPersistence;
#[cfg(all(not(target_arch = "wasm32"), feature = "persist-file"))]
pub use file::FilePersistence;

#[cfg(all(target_arch = "wasm32", feature = "persist-libsql"))]
pub use libsql::LibsqlPersistence;

use records::{
    Alias, AliasInput, AuditLog, AuditLogInput, Credential, CredentialInput, CredentialStatus,
    CredentialStatusInput, DownstreamRequest, DownstreamRequestInput, InstanceSettings,
    InstanceSettingsInput, Org, OrgInput, Provider, ProviderInput, ProviderModel,
    ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, Quota, QuotaInput, RateLimit,
    RateLimitInput, Route, RouteInput, RouteMember, RouteMemberInput, RoutePermission,
    RoutePermissionInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
    Scope, Team, TeamInput, UpstreamRequest, UpstreamRequestInput, Usage, UsageInput, UsageRollup,
    UsageRollupInput, User, UserInput, UserKey, UserKeyInput,
};

/// Durable storage abstraction.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait PersistenceBackend: Send + Sync {
    /// Backend kind label for diagnostics: "file" | "db" | "libsql".
    fn kind(&self) -> &'static str;

    /// Verify the backend is reachable/usable.
    async fn health(&self) -> anyhow::Result<()>;

    // ── providers (§8-B) ────────────────────────────────────────────────────

    /// List all providers.
    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>>;

    /// Fetch a provider by id, or `None` if absent.
    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>>;

    /// Fetch a provider by its unique name, or `None` if absent.
    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>>;

    /// Insert (`input.id == None`) or update (`Some(id)`) a provider; returns
    /// the stored record with id and timestamps populated.
    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider>;

    /// Delete a provider by id; returns whether a row was removed.
    /// Application-level cascade: also removes the provider's credentials
    /// (and their statuses) and provider models.
    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool>;

    // ── credentials (§8-B) ──────────────────────────────────────────────────

    /// List credentials in a provider's pool.
    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>>;

    /// Fetch a credential by id.
    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>>;

    /// Insert or update a credential.
    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential>;

    /// Refresh-only compare-and-set: update only `secret_json` when the
    /// credential still exists, belongs to the same provider, is enabled, and
    /// has not changed since `expected_updated_at`.
    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool>;

    /// Delete a credential; cascades to its status snapshots.
    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool>;

    /// List a credential's health snapshots.
    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>>;

    /// List ALL credential health snapshots (batch endpoint — B5).
    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>>;

    /// Insert or update a credential status (unique per `(credential_id, channel)`).
    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus>;

    /// Delete a credential status by id.
    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool>;

    // ── routes / members / aliases (§8-A) ───────────────────────────────────

    /// List all routes.
    async fn list_routes(&self) -> anyhow::Result<Vec<Route>>;

    /// Fetch a route by id.
    async fn get_route(&self, id: i64) -> anyhow::Result<Option<Route>>;

    /// Fetch a route by its unique name.
    async fn get_route_by_name(&self, name: &str) -> anyhow::Result<Option<Route>>;

    /// Insert or update a route.
    async fn upsert_route(&self, input: RouteInput) -> anyhow::Result<Route>;

    /// Delete a route; cascades to its members and aliases.
    async fn delete_route(&self, id: i64) -> anyhow::Result<bool>;

    /// List a route's members.
    async fn list_route_members(&self, route_id: i64) -> anyhow::Result<Vec<RouteMember>>;

    /// Insert or update a route member.
    async fn upsert_route_member(&self, input: RouteMemberInput) -> anyhow::Result<RouteMember>;

    /// Delete a route member by id.
    async fn delete_route_member(&self, id: i64) -> anyhow::Result<bool>;

    /// List all aliases.
    async fn list_aliases(&self) -> anyhow::Result<Vec<Alias>>;

    /// Fetch an alias by its unique alias name.
    async fn get_alias_by_name(&self, alias: &str) -> anyhow::Result<Option<Alias>>;

    /// Insert or update an alias.
    async fn upsert_alias(&self, input: AliasInput) -> anyhow::Result<Alias>;

    /// Delete an alias by id.
    async fn delete_alias(&self, id: i64) -> anyhow::Result<bool>;

    // ── provider models (§8-A) ──────────────────────────────────────────────

    /// List a provider's models.
    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>>;

    /// Insert or update a provider model.
    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel>;

    /// Delete a provider model by id.
    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool>;

    // ── routing rules (§8-B) ─────────────────────────────────────────────────

    /// List a provider's routing rules.
    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>>;

    /// Fetch a routing rule by id.
    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>>;

    /// Insert or update a routing rule (unique per `(provider_id, operation, kind)`).
    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule>;

    /// Delete a routing rule by id.
    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool>;

    // ── rule sets (§8-B2) ────────────────────────────────────────────────────

    /// List all reusable rule sets.
    async fn list_rule_sets(&self) -> anyhow::Result<Vec<RuleSet>>;

    /// Fetch a rule set by id.
    async fn get_rule_set(&self, id: i64) -> anyhow::Result<Option<RuleSet>>;

    /// Fetch a rule set by its unique name.
    async fn get_rule_set_by_name(&self, name: &str) -> anyhow::Result<Option<RuleSet>>;

    /// Insert or update a rule set (unique `name`).
    async fn upsert_rule_set(&self, input: RuleSetInput) -> anyhow::Result<RuleSet>;

    /// Delete a rule set by id; cascades to its rules and its provider
    /// attachments (the shared rule set's providers are untouched).
    async fn delete_rule_set(&self, id: i64) -> anyhow::Result<bool>;

    // ── rules (§8-B2) ────────────────────────────────────────────────────────

    /// List a rule set's rules.
    async fn list_rules(&self, rule_set_id: i64) -> anyhow::Result<Vec<Rule>>;

    /// Fetch a rule by id.
    async fn get_rule(&self, id: i64) -> anyhow::Result<Option<Rule>>;

    /// Insert or update a rule.
    async fn upsert_rule(&self, input: RuleInput) -> anyhow::Result<Rule>;

    /// Delete a rule by id.
    async fn delete_rule(&self, id: i64) -> anyhow::Result<bool>;

    // ── provider rule sets (§8-B2) ───────────────────────────────────────────

    /// List a provider's rule-set attachments.
    async fn list_provider_rule_sets(
        &self,
        provider_id: i64,
    ) -> anyhow::Result<Vec<ProviderRuleSet>>;

    /// Insert or update a provider ↔ rule-set attachment.
    async fn upsert_provider_rule_set(
        &self,
        input: ProviderRuleSetInput,
    ) -> anyhow::Result<ProviderRuleSet>;

    /// Delete a provider rule-set attachment by id.
    async fn delete_provider_rule_set(&self, id: i64) -> anyhow::Result<bool>;

    // ── orgs (§8-C) ─────────────────────────────────────────────────────────

    /// List all orgs.
    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>>;

    /// Fetch an org by id.
    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>>;

    /// Fetch an org by its unique name.
    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>>;

    /// Insert or update an org.
    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org>;

    /// Delete an org; cascades to its teams, users (and their keys), and any
    /// org-scoped route permissions, rate limits, and quotas.
    async fn delete_org(&self, id: i64) -> anyhow::Result<bool>;

    // ── teams (§8-C) ────────────────────────────────────────────────────────

    /// List a team's siblings within an org.
    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>>;

    /// Fetch a team by id.
    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>>;

    /// Insert or update a team (unique per `(org_id, name)`).
    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team>;

    /// Delete a team; detaches its members and drops team-scoped route
    /// permissions, rate limits, and quotas.
    async fn delete_team(&self, id: i64) -> anyhow::Result<bool>;

    // ── users (§8-C) ────────────────────────────────────────────────────────

    /// List all users.
    async fn list_users(&self) -> anyhow::Result<Vec<User>>;

    /// Fetch a user by id.
    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>>;

    /// Fetch a user by their unique name.
    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>>;

    /// Insert or update a user.
    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User>;

    /// Delete a user; cascades to their keys and user-scoped route permissions,
    /// rate limits, and quotas.
    async fn delete_user(&self, id: i64) -> anyhow::Result<bool>;

    // ── user keys (§8-C) ────────────────────────────────────────────────────

    /// List a user's API keys.
    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>>;

    /// Fetch a user key by id.
    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>>;

    /// Find a user key by its unique digest.
    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>>;

    /// Insert or update a user key.
    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey>;

    /// Delete a user key by id.
    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool>;

    // ── authz: route permissions / rate limits / quotas (§8-C) ──────────────

    /// List route permissions for a scope (org/team/user).
    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>>;

    /// Insert or update a route permission.
    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission>;

    /// Delete a route permission by id.
    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool>;

    /// List rate limits for a scope.
    async fn list_rate_limits(&self, scope: Scope, scope_id: i64)
    -> anyhow::Result<Vec<RateLimit>>;

    /// Insert or update a rate limit.
    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit>;

    /// Delete a rate limit by id.
    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool>;

    /// Fetch the quota for a scope (unique per `(scope, scope_id)`).
    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>>;

    /// Insert or update a quota.
    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota>;

    /// Delete a quota by id.
    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool>;

    /// Atomically add `delta` to the `cost_used` of the quota row for
    /// `(scope, scope_id)`. No-op (Ok) if no such row exists — a request whose
    /// identity has no quota row simply isn't cost-tracked. Closes the M6
    /// read-modify-write reconcile race: the increment is applied under the
    /// backend's own atomicity (file: write lock; db: a single transaction).
    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()>;

    // ── usage / logs (§8-D) ─────────────────────────────────────────────────

    /// Append a per-request usage row (append-only). Idempotent by
    /// `request_id`: returns `Ok(None)` without writing when a row with the
    /// same `request_id` already exists (§17 settle-exactly-once).
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>>;

    /// List the most recent usage rows (by id desc), up to `limit`.
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>>;

    /// Filtered + keyset-paginated usage rows for the usage explorer (B4). Rows
    /// are returned `id` DESC; `q.before_id` is the cursor (`id < before_id`).
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>>;

    /// Accumulate metric deltas into the rollup bucket identified by the input's
    /// `(granularity, bucket_start, dimensions)`; creates the bucket if absent.
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup>;

    /// List rollup buckets for `granularity` with `bucket_start` in `[from, to]`.
    /// When `user_id` is `Some(v)` only buckets whose `user_id == v` are returned;
    /// `None` returns all buckets (admin / billing paths).
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>>;

    /// §15.3: a persistence-derived metrics snapshot (token/request totals from
    /// rollups, upstream-latency histogram from usages, credential-health
    /// counts, per-scope quota gauges) for the `/metrics` endpoint. Computed by
    /// backend-side aggregate queries, never in-memory counters.
    async fn metrics_aggregate(
        &self,
    ) -> anyhow::Result<crate::store::persistence::metrics::MetricsAggregate>;

    /// Append a raw downstream (client → proxy) request log entry.
    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest>;

    /// List downstream request log entries for a `request_id`.
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>>;

    /// Recent downstream request logs across all requests, keyset-paginated `id`
    /// DESC (`before_id` cursor). Powers the Logs explorer.
    async fn list_recent_downstream_requests(
        &self,
        limit: u64,
        before_id: Option<i64>,
    ) -> anyhow::Result<Vec<DownstreamRequest>>;

    /// Backfill the captured client-facing response body onto an existing
    /// `downstream_requests` row (streaming responses settle after the row is
    /// appended). No-op when no row matches `request_id`.
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()>;

    /// Append a raw upstream (proxy → provider) request log entry.
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest>;

    /// List upstream request log entries for a `request_id`.
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>>;

    /// Backfill the captured upstream (provider) response body onto an existing
    /// `upstream_requests` row. No-op when no row matches `request_id`.
    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()>;

    /// Purge append-only usage/request-log rows older than `cutoff_ts`
    /// (`created_at < cutoff_ts`; §8-D retention). Covers the high-volume raw
    /// tables — `usages`, `downstream_requests`, `upstream_requests` — and keeps
    /// the compact `usage_rollups` aggregates and `audit_logs`. Returns the total
    /// rows removed. Edge isolates are short-lived, so the edge backend is a
    /// no-op (server-side cleanup is expected).
    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64>;

    // ── admin audit log (§ admin hardening) ─────────────────────────────────

    /// Append an audit row (append-only); returns it with id/created_at set.
    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog>;

    /// List the most recent audit rows (by id desc), up to `limit`.
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>>;

    // ── instance settings (§8) ──────────────────────────────────────────────

    /// List all instance settings.
    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>>;

    /// Fetch instance settings by their unique `instance_name`, or `None`.
    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>>;

    /// Insert or update instance settings (unique per `instance_name`).
    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings>;

    // ── tokenizer vocabs (§6.3) ─────────────────────────────────────────────

    /// Stored tokenizer vocabularies (HF tokenizer.json blobs). Defaults are
    /// for backends without vocab storage (edge): empty/unsupported.
    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }

    /// Fetch a stored vocab's raw bytes by name, or `None` if absent.
    async fn get_tokenizer_vocab(&self, _name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    /// Store (insert or replace) a vocab's raw bytes under `name`.
    async fn put_tokenizer_vocab(&self, _name: &str, _bytes: &[u8]) -> anyhow::Result<()> {
        anyhow::bail!("tokenizer vocab storage unsupported by this backend")
    }
}
