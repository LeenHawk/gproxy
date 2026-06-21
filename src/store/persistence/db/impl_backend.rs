//! `PersistenceBackend` implementation for [`DbPersistence`].

use async_trait::async_trait;

use super::DbPersistence;
use super::ops;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::UsageQuery;
use crate::store::persistence::records::{
    Alias, AliasInput, AuditLog, AuditLogInput, Credential, CredentialInput, CredentialStatus,
    CredentialStatusInput, DownstreamRequest, DownstreamRequestInput, InstanceSettings,
    InstanceSettingsInput, Org, OrgInput, Provider, ProviderInput, ProviderModel,
    ProviderModelInput, ProviderRuleSet, ProviderRuleSetInput, Quota, QuotaInput, RateLimit,
    RateLimitInput, Route, RouteInput, RouteMember, RouteMemberInput, RoutePermission,
    RoutePermissionInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
    Scope, Team, TeamInput, UpstreamRequest, UpstreamRequestInput, Usage, UsageInput, UsageRollup,
    UsageRollupInput, User, UserInput, UserKey, UserKeyInput,
};

#[async_trait]
impl PersistenceBackend for DbPersistence {
    fn kind(&self) -> &'static str {
        "db"
    }

    async fn health(&self) -> anyhow::Result<()> {
        self.conn
            .ping()
            .await
            .map_err(|e| anyhow::anyhow!("db ping failed: {e}"))
    }

    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>> {
        ops::provider::providers::list(&self.conn).await
    }

    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>> {
        ops::provider::providers::get(&self.conn, id).await
    }

    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>> {
        ops::provider::providers::get_by_name(&self.conn, name).await
    }

    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider> {
        ops::provider::providers::upsert(&self.conn, input).await
    }

    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool> {
        ops::provider::providers::delete(&self.conn, id).await
    }

    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
        ops::provider::credentials::list(&self.conn, provider_id).await
    }

    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>> {
        ops::provider::credentials::get(&self.conn, id).await
    }

    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential> {
        ops::provider::credentials::upsert(&self.conn, input).await
    }

    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool> {
        ops::provider::credentials::update_secret_if_current(
            &self.conn,
            id,
            provider_id,
            expected_updated_at,
            secret_json,
        )
        .await
    }

    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool> {
        ops::provider::credentials::delete(&self.conn, id).await
    }

    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>> {
        ops::provider::credential_statuses::list(&self.conn, credential_id).await
    }

    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>> {
        ops::provider::credential_statuses::list_all(&self.conn).await
    }

    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus> {
        ops::provider::credential_statuses::upsert(&self.conn, input).await
    }

    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool> {
        ops::provider::credential_statuses::delete(&self.conn, id).await
    }

    async fn list_routes(&self) -> anyhow::Result<Vec<Route>> {
        ops::routing::routes::list(&self.conn).await
    }

    async fn get_route(&self, id: i64) -> anyhow::Result<Option<Route>> {
        ops::routing::routes::get(&self.conn, id).await
    }

    async fn get_route_by_name(&self, name: &str) -> anyhow::Result<Option<Route>> {
        ops::routing::routes::get_by_name(&self.conn, name).await
    }

    async fn upsert_route(&self, input: RouteInput) -> anyhow::Result<Route> {
        ops::routing::routes::upsert(&self.conn, input).await
    }

    async fn delete_route(&self, id: i64) -> anyhow::Result<bool> {
        ops::routing::routes::delete(&self.conn, id).await
    }

    async fn list_route_members(&self, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
        ops::routing::route_members::list(&self.conn, route_id).await
    }

    async fn upsert_route_member(&self, input: RouteMemberInput) -> anyhow::Result<RouteMember> {
        ops::routing::route_members::upsert(&self.conn, input).await
    }

    async fn delete_route_member(&self, id: i64) -> anyhow::Result<bool> {
        ops::routing::route_members::delete(&self.conn, id).await
    }

    async fn list_aliases(&self) -> anyhow::Result<Vec<Alias>> {
        ops::routing::aliases::list(&self.conn).await
    }

    async fn get_alias_by_name(&self, alias: &str) -> anyhow::Result<Option<Alias>> {
        ops::routing::aliases::get_by_name(&self.conn, alias).await
    }

    async fn upsert_alias(&self, input: AliasInput) -> anyhow::Result<Alias> {
        ops::routing::aliases::upsert(&self.conn, input).await
    }

    async fn delete_alias(&self, id: i64) -> anyhow::Result<bool> {
        ops::routing::aliases::delete(&self.conn, id).await
    }

    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
        ops::provider::provider_models::list(&self.conn, provider_id).await
    }

    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel> {
        ops::provider::provider_models::upsert(&self.conn, input).await
    }

    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool> {
        ops::provider::provider_models::delete(&self.conn, id).await
    }

    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
        ops::transform::routing_rules::list(&self.conn, provider_id).await
    }

    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>> {
        ops::transform::routing_rules::get(&self.conn, id).await
    }

    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
        ops::transform::routing_rules::upsert(&self.conn, input).await
    }

    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool> {
        ops::transform::routing_rules::delete(&self.conn, id).await
    }

    async fn list_rule_sets(&self) -> anyhow::Result<Vec<RuleSet>> {
        ops::transform::rule_sets::list(&self.conn).await
    }

    async fn get_rule_set(&self, id: i64) -> anyhow::Result<Option<RuleSet>> {
        ops::transform::rule_sets::get(&self.conn, id).await
    }

    async fn get_rule_set_by_name(&self, name: &str) -> anyhow::Result<Option<RuleSet>> {
        ops::transform::rule_sets::get_by_name(&self.conn, name).await
    }

    async fn upsert_rule_set(&self, input: RuleSetInput) -> anyhow::Result<RuleSet> {
        ops::transform::rule_sets::upsert(&self.conn, input).await
    }

    async fn delete_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        ops::transform::rule_sets::delete(&self.conn, id).await
    }

    async fn list_rules(&self, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
        ops::transform::rules::list(&self.conn, rule_set_id).await
    }

    async fn get_rule(&self, id: i64) -> anyhow::Result<Option<Rule>> {
        ops::transform::rules::get(&self.conn, id).await
    }

    async fn upsert_rule(&self, input: RuleInput) -> anyhow::Result<Rule> {
        ops::transform::rules::upsert(&self.conn, input).await
    }

    async fn delete_rule(&self, id: i64) -> anyhow::Result<bool> {
        ops::transform::rules::delete(&self.conn, id).await
    }

    async fn list_provider_rule_sets(
        &self,
        provider_id: i64,
    ) -> anyhow::Result<Vec<ProviderRuleSet>> {
        ops::transform::provider_rule_sets::list(&self.conn, provider_id).await
    }

    async fn upsert_provider_rule_set(
        &self,
        input: ProviderRuleSetInput,
    ) -> anyhow::Result<ProviderRuleSet> {
        ops::transform::provider_rule_sets::upsert(&self.conn, input).await
    }

    async fn delete_provider_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        ops::transform::provider_rule_sets::delete(&self.conn, id).await
    }

    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>> {
        ops::identity::orgs::list(&self.conn).await
    }

    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>> {
        ops::identity::orgs::get(&self.conn, id).await
    }

    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>> {
        ops::identity::orgs::get_by_name(&self.conn, name).await
    }

    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org> {
        ops::identity::orgs::upsert(&self.conn, input).await
    }

    async fn delete_org(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::orgs::delete(&self.conn, id).await
    }

    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>> {
        ops::identity::teams::list(&self.conn, org_id).await
    }

    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>> {
        ops::identity::teams::get(&self.conn, id).await
    }

    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team> {
        ops::identity::teams::upsert(&self.conn, input).await
    }

    async fn delete_team(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::teams::delete(&self.conn, id).await
    }

    async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        ops::identity::users::list(&self.conn).await
    }

    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>> {
        ops::identity::users::get(&self.conn, id).await
    }

    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>> {
        ops::identity::users::get_by_name(&self.conn, name).await
    }

    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User> {
        ops::identity::users::upsert(&self.conn, input).await
    }

    async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::users::delete(&self.conn, id).await
    }

    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
        ops::identity::user_keys::list(&self.conn, user_id).await
    }

    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>> {
        ops::identity::user_keys::get(&self.conn, id).await
    }

    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>> {
        ops::identity::user_keys::find_by_digest(&self.conn, digest).await
    }

    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey> {
        ops::identity::user_keys::upsert(&self.conn, input).await
    }

    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::user_keys::delete(&self.conn, id).await
    }

    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>> {
        ops::authz::route_permissions::list(&self.conn, scope, scope_id).await
    }

    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission> {
        ops::authz::route_permissions::upsert(&self.conn, input).await
    }

    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::route_permissions::delete(&self.conn, id).await
    }

    async fn list_rate_limits(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RateLimit>> {
        ops::authz::rate_limits::list(&self.conn, scope, scope_id).await
    }

    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit> {
        ops::authz::rate_limits::upsert(&self.conn, input).await
    }

    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::rate_limits::delete(&self.conn, id).await
    }

    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
        ops::authz::quotas::get(&self.conn, scope, scope_id).await
    }

    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota> {
        ops::authz::quotas::upsert(&self.conn, input).await
    }

    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::quotas::delete(&self.conn, id).await
    }

    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        ops::authz::quotas::add_cost(&self.conn, scope, scope_id, delta).await
    }

    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        ops::usage::usages::append(&self.conn, input).await
    }

    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        ops::usage::usages::list(&self.conn, limit).await
    }

    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        ops::usage::usages::query(&self.conn, q).await
    }

    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        ops::usage::usage_rollups::add(&self.conn, input).await
    }

    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        ops::usage::usage_rollups::list(&self.conn, granularity, from, to, user_id).await
    }

    async fn metrics_aggregate(
        &self,
    ) -> anyhow::Result<crate::store::persistence::metrics::MetricsAggregate> {
        ops::metrics::aggregate(&self.conn).await
    }

    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        ops::logs::downstream_requests::append(&self.conn, input).await
    }

    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        ops::logs::downstream_requests::list(&self.conn, request_id).await
    }

    async fn list_recent_downstream_requests(
        &self,
        limit: u64,
        before_id: Option<i64>,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        ops::logs::downstream_requests::list_recent(&self.conn, limit, before_id).await
    }

    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        ops::logs::downstream_requests::update_response_body(&self.conn, request_id, response_body)
            .await
    }

    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        ops::logs::upstream_requests::append(&self.conn, input).await
    }

    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        ops::logs::upstream_requests::list(&self.conn, request_id).await
    }

    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        ops::logs::upstream_requests::update_response_body(&self.conn, request_id, response_body)
            .await
    }

    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        use crate::store::persistence::db::entities::{logs, usage};
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let mut removed = 0u64;
        removed += usage::usage::Entity::delete_many()
            .filter(usage::usage::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        removed += logs::downstream_request::Entity::delete_many()
            .filter(logs::downstream_request::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        removed += logs::upstream_request::Entity::delete_many()
            .filter(logs::upstream_request::Column::CreatedAt.lt(cutoff_ts))
            .exec(&self.conn)
            .await?
            .rows_affected;
        Ok(removed)
    }

    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        ops::logs::audit_logs::append(&self.conn, input).await
    }

    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        ops::logs::audit_logs::list(&self.conn, limit).await
    }

    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>> {
        ops::settings::instance_settings::list(&self.conn).await
    }

    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>> {
        ops::settings::instance_settings::get(&self.conn, instance_name).await
    }

    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings> {
        ops::settings::instance_settings::upsert(&self.conn, input).await
    }

    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        ops::tokenize::tokenizer_vocabs::list(&self.conn).await
    }

    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        ops::tokenize::tokenizer_vocabs::get(&self.conn, name).await
    }

    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        ops::tokenize::tokenizer_vocabs::put(&self.conn, name, bytes).await
    }
}
