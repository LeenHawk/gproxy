//! Edge (wasm32) persistence backend backed by libSQL/Turso over Hrana HTTP.
//!
//! `LibsqlPersistence` wraps [`LibsqlClient`] and implements
//! [`PersistenceBackend`] with hand-written SQLite SQL, mirroring the native
//! `db` (SeaORM) backend's semantics. `connect` ensures the schema exists.
//!
//! Compile-checked on wasm32; round-trips require Turso credentials
//! (see the ignored integration test).

mod authz;
mod identity;
mod logs;
mod metrics;
mod provider;
mod routing;
mod row;
mod schema;
mod settings;
mod tokenize;
mod transform;
mod usage;
mod util;

use crate::store::libsql::LibsqlClient;

use super::PersistenceBackend;
use super::UsageQuery;
use super::metrics::MetricsAggregate;
use super::records::{
    Alias, AliasInput, AuditLog, AuditLogInput, Credential, CredentialInput, CredentialStatus,
    CredentialStatusInput, DownstreamRequest, DownstreamRequestInput, InstanceSettings,
    InstanceSettingsInput, Org, OrgInput, Provider, ProviderInput, ProviderModel,
    ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, Quota, QuotaInput, RateLimit,
    RateLimitInput, Route, RouteInput, RouteMember, RouteMemberInput, RoutePermission,
    RoutePermissionInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
    Scope, Team, TeamInput, UpstreamRequest, UpstreamRequestInput, Usage, UsageInput, UsageRollup,
    UsageRollupInput, User, UserInput, UserKey, UserKeyInput,
};

/// Edge persistence backend backed by a Turso/libSQL database via Hrana HTTP.
pub struct LibsqlPersistence {
    client: LibsqlClient,
}

impl LibsqlPersistence {
    /// Create a new persistence backend and ensure the schema exists.
    ///
    /// `url` — Turso database URL (e.g. `https://<db>.turso.io`).
    /// `token` — Bearer auth token.
    pub async fn connect(url: String, token: String) -> anyhow::Result<Self> {
        let client = LibsqlClient::new(url, token);
        schema::ensure_schema(&client).await?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait(?Send)]
impl PersistenceBackend for LibsqlPersistence {
    fn kind(&self) -> &'static str {
        "libsql"
    }

    async fn health(&self) -> anyhow::Result<()> {
        self.client
            .execute("SELECT 1", &[])
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("libsql health failed: {e}"))
    }

    // ── providers ──
    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>> {
        provider::providers::list(&self.client).await
    }
    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>> {
        provider::providers::get(&self.client, id).await
    }
    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>> {
        provider::providers::get_by_name(&self.client, name).await
    }
    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider> {
        provider::providers::upsert(&self.client, input).await
    }
    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool> {
        provider::providers::delete(&self.client, id).await
    }

    // ── credentials ──
    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
        provider::credentials::list(&self.client, provider_id).await
    }
    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>> {
        provider::credentials::get(&self.client, id).await
    }
    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential> {
        provider::credentials::upsert(&self.client, input).await
    }
    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool> {
        provider::credentials::update_secret_if_current(
            &self.client,
            id,
            provider_id,
            expected_updated_at,
            secret_json,
        )
        .await
    }
    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool> {
        provider::credentials::delete(&self.client, id).await
    }
    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>> {
        provider::credential_statuses::list(&self.client, credential_id).await
    }
    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>> {
        provider::credential_statuses::list_all(&self.client).await
    }
    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus> {
        provider::credential_statuses::upsert(&self.client, input).await
    }
    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool> {
        provider::credential_statuses::delete(&self.client, id).await
    }

    // ── routes / members / aliases ──
    async fn list_routes(&self) -> anyhow::Result<Vec<Route>> {
        routing::routes::list(&self.client).await
    }
    async fn get_route(&self, id: i64) -> anyhow::Result<Option<Route>> {
        routing::routes::get(&self.client, id).await
    }
    async fn get_route_by_name(&self, name: &str) -> anyhow::Result<Option<Route>> {
        routing::routes::get_by_name(&self.client, name).await
    }
    async fn upsert_route(&self, input: RouteInput) -> anyhow::Result<Route> {
        routing::routes::upsert(&self.client, input).await
    }
    async fn delete_route(&self, id: i64) -> anyhow::Result<bool> {
        routing::routes::delete(&self.client, id).await
    }
    async fn list_route_members(&self, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
        routing::route_members::list(&self.client, route_id).await
    }
    async fn upsert_route_member(&self, input: RouteMemberInput) -> anyhow::Result<RouteMember> {
        routing::route_members::upsert(&self.client, input).await
    }
    async fn delete_route_member(&self, id: i64) -> anyhow::Result<bool> {
        routing::route_members::delete(&self.client, id).await
    }
    async fn list_aliases(&self) -> anyhow::Result<Vec<Alias>> {
        routing::aliases::list(&self.client).await
    }
    async fn get_alias_by_name(&self, alias: &str) -> anyhow::Result<Option<Alias>> {
        routing::aliases::get_by_name(&self.client, alias).await
    }
    async fn upsert_alias(&self, input: AliasInput) -> anyhow::Result<Alias> {
        routing::aliases::upsert(&self.client, input).await
    }
    async fn delete_alias(&self, id: i64) -> anyhow::Result<bool> {
        routing::aliases::delete(&self.client, id).await
    }

    // ── provider models ──
    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
        provider::provider_models::list(&self.client, provider_id).await
    }
    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel> {
        provider::provider_models::upsert(&self.client, input).await
    }
    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool> {
        provider::provider_models::delete(&self.client, id).await
    }

    // ── routing rules ──
    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
        transform::routing_rules::list(&self.client, provider_id).await
    }
    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>> {
        transform::routing_rules::get(&self.client, id).await
    }
    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
        transform::routing_rules::upsert(&self.client, input).await
    }
    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool> {
        transform::routing_rules::delete(&self.client, id).await
    }

    // ── rule sets ──
    async fn list_rule_sets(&self) -> anyhow::Result<Vec<RuleSet>> {
        transform::rule_sets::list(&self.client).await
    }
    async fn get_rule_set(&self, id: i64) -> anyhow::Result<Option<RuleSet>> {
        transform::rule_sets::get(&self.client, id).await
    }
    async fn get_rule_set_by_name(&self, name: &str) -> anyhow::Result<Option<RuleSet>> {
        transform::rule_sets::get_by_name(&self.client, name).await
    }
    async fn upsert_rule_set(&self, input: RuleSetInput) -> anyhow::Result<RuleSet> {
        transform::rule_sets::upsert(&self.client, input).await
    }
    async fn delete_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        transform::rule_sets::delete(&self.client, id).await
    }

    // ── rules ──
    async fn list_rules(&self, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
        transform::rules::list(&self.client, rule_set_id).await
    }
    async fn get_rule(&self, id: i64) -> anyhow::Result<Option<Rule>> {
        transform::rules::get(&self.client, id).await
    }
    async fn upsert_rule(&self, input: RuleInput) -> anyhow::Result<Rule> {
        transform::rules::upsert(&self.client, input).await
    }
    async fn delete_rule(&self, id: i64) -> anyhow::Result<bool> {
        transform::rules::delete(&self.client, id).await
    }

    // ── provider rule sets ──
    async fn list_provider_rule_sets(
        &self,
        provider_id: i64,
    ) -> anyhow::Result<Vec<ProviderRuleSet>> {
        transform::provider_rule_sets::list(&self.client, provider_id).await
    }
    async fn upsert_provider_rule_set(
        &self,
        input: ProviderRuleSetInput,
    ) -> anyhow::Result<ProviderRuleSet> {
        transform::provider_rule_sets::upsert(&self.client, input).await
    }
    async fn delete_provider_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        transform::provider_rule_sets::delete(&self.client, id).await
    }

    // ── orgs ──
    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>> {
        identity::orgs::list(&self.client).await
    }
    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>> {
        identity::orgs::get(&self.client, id).await
    }
    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>> {
        identity::orgs::get_by_name(&self.client, name).await
    }
    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org> {
        identity::orgs::upsert(&self.client, input).await
    }
    async fn delete_org(&self, id: i64) -> anyhow::Result<bool> {
        identity::orgs::delete(&self.client, id).await
    }

    // ── teams ──
    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>> {
        identity::teams::list(&self.client, org_id).await
    }
    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>> {
        identity::teams::get(&self.client, id).await
    }
    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team> {
        identity::teams::upsert(&self.client, input).await
    }
    async fn delete_team(&self, id: i64) -> anyhow::Result<bool> {
        identity::teams::delete(&self.client, id).await
    }

    // ── users ──
    async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        identity::users::list(&self.client).await
    }
    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>> {
        identity::users::get(&self.client, id).await
    }
    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>> {
        identity::users::get_by_name(&self.client, name).await
    }
    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User> {
        identity::users::upsert(&self.client, input).await
    }
    async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        identity::users::delete(&self.client, id).await
    }

    // ── user keys ──
    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
        identity::user_keys::list(&self.client, user_id).await
    }
    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>> {
        identity::user_keys::get(&self.client, id).await
    }
    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>> {
        identity::user_keys::find_by_digest(&self.client, digest).await
    }
    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey> {
        identity::user_keys::upsert(&self.client, input).await
    }
    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool> {
        identity::user_keys::delete(&self.client, id).await
    }

    // ── authz ──
    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>> {
        authz::route_permissions::list(&self.client, scope, scope_id).await
    }
    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission> {
        authz::route_permissions::upsert(&self.client, input).await
    }
    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool> {
        authz::route_permissions::delete(&self.client, id).await
    }
    async fn list_rate_limits(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RateLimit>> {
        authz::rate_limits::list(&self.client, scope, scope_id).await
    }
    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit> {
        authz::rate_limits::upsert(&self.client, input).await
    }
    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool> {
        authz::rate_limits::delete(&self.client, id).await
    }
    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
        authz::quotas::get(&self.client, scope, scope_id).await
    }
    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota> {
        authz::quotas::upsert(&self.client, input).await
    }
    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool> {
        authz::quotas::delete(&self.client, id).await
    }
    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        authz::quotas::add_cost(&self.client, scope, scope_id, delta).await
    }

    // ── usage / logs ──
    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        usage::usages::append(&self.client, input).await
    }
    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        usage::usages::list(&self.client, limit).await
    }
    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        usage::usages::query(&self.client, q).await
    }
    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        usage::usage_rollups::add(&self.client, input).await
    }
    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        usage::usage_rollups::list(&self.client, granularity, from, to, user_id).await
    }
    async fn metrics_aggregate(&self) -> anyhow::Result<MetricsAggregate> {
        metrics::aggregate(&self.client).await
    }
    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        logs::downstream_requests::append(&self.client, input).await
    }
    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        logs::downstream_requests::list(&self.client, request_id).await
    }
    async fn list_recent_downstream_requests(
        &self,
        limit: u64,
        before_id: Option<i64>,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        logs::downstream_requests::list_recent(&self.client, limit, before_id).await
    }
    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        logs::downstream_requests::update_response_body(&self.client, request_id, response_body)
            .await
    }
    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        logs::upstream_requests::append(&self.client, input).await
    }
    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        logs::upstream_requests::list(&self.client, request_id).await
    }
    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        logs::upstream_requests::update_response_body(&self.client, request_id, response_body).await
    }

    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        use crate::store::libsql::arg_integer;
        use crate::store::persistence::libsql::util::exec;
        let mut removed = 0u64;
        for table in ["usages", "downstream_requests", "upstream_requests"] {
            removed += exec(
                &self.client,
                &format!("DELETE FROM {table} WHERE created_at < ?"),
                &[arg_integer(cutoff_ts)],
            )
            .await?;
        }
        Ok(removed)
    }

    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        logs::audit_logs::append(&self.client, input).await
    }
    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        logs::audit_logs::list(&self.client, limit).await
    }

    // ── instance settings ──
    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>> {
        settings::instance_settings::list(&self.client).await
    }
    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>> {
        settings::instance_settings::get(&self.client, instance_name).await
    }
    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings> {
        settings::instance_settings::upsert(&self.client, input).await
    }

    // ── tokenizer vocabs ──
    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        tokenize::tokenizer_vocabs::list(&self.client).await
    }
    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        tokenize::tokenizer_vocabs::get(&self.client, name).await
    }
    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        tokenize::tokenizer_vocabs::put(&self.client, name, bytes).await
    }
}

/// Map a libsql/Hrana error from an insert/update: a SQLite UNIQUE-constraint
/// violation becomes a [`crate::store::persistence::ConflictError`] (→ 409
/// once the edge admin layer maps it); anything else passes through as an
/// anyhow error.
pub(crate) fn conflict_if_unique(
    e: crate::store::libsql::StoreError,
    msg: impl FnOnce() -> String,
) -> anyhow::Error {
    if e.to_string().contains("UNIQUE constraint failed") {
        crate::store::persistence::ConflictError::new(msg()).into()
    } else {
        anyhow::anyhow!("libsql: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wasm_bindgen_test::wasm_bindgen_test]
    #[ignore = "requires live Turso creds via GPROXY_TEST_TURSO_URL / GPROXY_TEST_TURSO_TOKEN"]
    async fn integration_health() {
        let url = std::env::var("GPROXY_TEST_TURSO_URL").expect("GPROXY_TEST_TURSO_URL");
        let token = std::env::var("GPROXY_TEST_TURSO_TOKEN").expect("GPROXY_TEST_TURSO_TOKEN");
        let backend = LibsqlPersistence::connect(url, token)
            .await
            .expect("connect");
        backend.health().await.expect("health");
    }
}
