//! `PersistenceBackend` implementation for [`FilePersistence`].

use async_trait::async_trait;

use super::FilePersistence;
use super::authz::{quotas, rate_limits, route_permissions};
use super::identity::{orgs, teams, user_keys, users};
use super::logs::{audit_logs, downstream_requests, upstream_requests};
use super::provider::{credential_statuses, credentials, provider_models, providers};
use super::routing::{aliases, route_members, routes};
use super::settings::instance_settings;
use super::tokenizers;
use super::transform::{provider_rule_sets, routing_rules, rule_sets, rules};
use super::usage::{usage_rollups, usages};
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
impl PersistenceBackend for FilePersistence {
    fn kind(&self) -> &'static str {
        "file"
    }

    async fn health(&self) -> anyhow::Result<()> {
        let probe = self
            .root
            .join(format!(".gproxy_health_{}", std::process::id()));
        tokio::fs::write(&probe, b"ok")
            .await
            .map_err(|e| anyhow::anyhow!("data dir is not writable: {e}"))?;
        tokio::fs::remove_file(&probe).await.ok();
        Ok(())
    }

    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>> {
        providers::list(&self.root).await
    }

    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>> {
        providers::get(&self.root, id).await
    }

    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>> {
        providers::get_by_name(&self.root, name).await
    }

    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider> {
        let _guard = self.write.lock().await;
        providers::upsert(&self.root, input).await
    }

    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        providers::delete(&self.root, id).await
    }

    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
        credentials::list(&self.root, provider_id).await
    }

    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>> {
        credentials::get(&self.root, id).await
    }

    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential> {
        let _guard = self.write.lock().await;
        credentials::upsert(&self.root, input).await
    }

    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        credentials::update_secret_if_current(
            &self.root,
            id,
            provider_id,
            expected_updated_at,
            secret_json,
        )
        .await
    }

    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        credentials::delete(&self.root, id).await
    }

    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>> {
        credential_statuses::list(&self.root, credential_id).await
    }

    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>> {
        credential_statuses::list_all(&self.root).await
    }

    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus> {
        let _guard = self.write.lock().await;
        credential_statuses::upsert(&self.root, input).await
    }

    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        credential_statuses::delete(&self.root, id).await
    }

    async fn list_routes(&self) -> anyhow::Result<Vec<Route>> {
        routes::list(&self.root).await
    }

    async fn get_route(&self, id: i64) -> anyhow::Result<Option<Route>> {
        routes::get(&self.root, id).await
    }

    async fn get_route_by_name(&self, name: &str) -> anyhow::Result<Option<Route>> {
        routes::get_by_name(&self.root, name).await
    }

    async fn upsert_route(&self, input: RouteInput) -> anyhow::Result<Route> {
        let _guard = self.write.lock().await;
        routes::upsert(&self.root, input).await
    }

    async fn delete_route(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        routes::delete(&self.root, id).await
    }

    async fn list_route_members(&self, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
        route_members::list(&self.root, route_id).await
    }

    async fn upsert_route_member(&self, input: RouteMemberInput) -> anyhow::Result<RouteMember> {
        let _guard = self.write.lock().await;
        route_members::upsert(&self.root, input).await
    }

    async fn delete_route_member(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        route_members::delete(&self.root, id).await
    }

    async fn list_aliases(&self) -> anyhow::Result<Vec<Alias>> {
        aliases::list(&self.root).await
    }

    async fn get_alias_by_name(&self, alias: &str) -> anyhow::Result<Option<Alias>> {
        aliases::get_by_name(&self.root, alias).await
    }

    async fn upsert_alias(&self, input: AliasInput) -> anyhow::Result<Alias> {
        let _guard = self.write.lock().await;
        aliases::upsert(&self.root, input).await
    }

    async fn delete_alias(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        aliases::delete(&self.root, id).await
    }

    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
        provider_models::list(&self.root, provider_id).await
    }

    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel> {
        let _guard = self.write.lock().await;
        provider_models::upsert(&self.root, input).await
    }

    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        provider_models::delete(&self.root, id).await
    }

    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
        routing_rules::list(&self.root, provider_id).await
    }

    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>> {
        routing_rules::get(&self.root, id).await
    }

    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
        let _guard = self.write.lock().await;
        routing_rules::upsert(&self.root, input).await
    }

    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        routing_rules::delete(&self.root, id).await
    }

    async fn list_rule_sets(&self) -> anyhow::Result<Vec<RuleSet>> {
        rule_sets::list(&self.root).await
    }

    async fn get_rule_set(&self, id: i64) -> anyhow::Result<Option<RuleSet>> {
        rule_sets::get(&self.root, id).await
    }

    async fn get_rule_set_by_name(&self, name: &str) -> anyhow::Result<Option<RuleSet>> {
        rule_sets::get_by_name(&self.root, name).await
    }

    async fn upsert_rule_set(&self, input: RuleSetInput) -> anyhow::Result<RuleSet> {
        let _guard = self.write.lock().await;
        rule_sets::upsert(&self.root, input).await
    }

    async fn delete_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        rule_sets::delete(&self.root, id).await
    }

    async fn list_rules(&self, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
        rules::list(&self.root, rule_set_id).await
    }

    async fn get_rule(&self, id: i64) -> anyhow::Result<Option<Rule>> {
        rules::get(&self.root, id).await
    }

    async fn upsert_rule(&self, input: RuleInput) -> anyhow::Result<Rule> {
        let _guard = self.write.lock().await;
        rules::upsert(&self.root, input).await
    }

    async fn delete_rule(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        rules::delete(&self.root, id).await
    }

    async fn list_provider_rule_sets(
        &self,
        provider_id: i64,
    ) -> anyhow::Result<Vec<ProviderRuleSet>> {
        provider_rule_sets::list(&self.root, provider_id).await
    }

    async fn upsert_provider_rule_set(
        &self,
        input: ProviderRuleSetInput,
    ) -> anyhow::Result<ProviderRuleSet> {
        let _guard = self.write.lock().await;
        provider_rule_sets::upsert(&self.root, input).await
    }

    async fn delete_provider_rule_set(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        provider_rule_sets::delete(&self.root, id).await
    }

    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>> {
        orgs::list(&self.root).await
    }

    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>> {
        orgs::get(&self.root, id).await
    }

    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>> {
        orgs::get_by_name(&self.root, name).await
    }

    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org> {
        let _guard = self.write.lock().await;
        orgs::upsert(&self.root, input).await
    }

    async fn delete_org(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        orgs::delete(&self.root, id).await
    }

    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>> {
        teams::list(&self.root, org_id).await
    }

    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>> {
        teams::get(&self.root, id).await
    }

    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team> {
        let _guard = self.write.lock().await;
        teams::upsert(&self.root, input).await
    }

    async fn delete_team(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        teams::delete(&self.root, id).await
    }

    async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        users::list(&self.root).await
    }

    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>> {
        users::get(&self.root, id).await
    }

    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>> {
        users::get_by_name(&self.root, name).await
    }

    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User> {
        let _guard = self.write.lock().await;
        users::upsert(&self.root, input).await
    }

    async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        users::delete(&self.root, id).await
    }

    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
        user_keys::list(&self.root, user_id).await
    }

    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>> {
        user_keys::get(&self.root, id).await
    }

    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>> {
        user_keys::find_by_digest(&self.root, digest).await
    }

    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey> {
        let _guard = self.write.lock().await;
        user_keys::upsert(&self.root, input).await
    }

    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        user_keys::delete(&self.root, id).await
    }

    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>> {
        route_permissions::list(&self.root, scope, scope_id).await
    }

    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission> {
        let _guard = self.write.lock().await;
        route_permissions::upsert(&self.root, input).await
    }

    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        route_permissions::delete(&self.root, id).await
    }

    async fn list_rate_limits(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RateLimit>> {
        rate_limits::list(&self.root, scope, scope_id).await
    }

    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit> {
        let _guard = self.write.lock().await;
        rate_limits::upsert(&self.root, input).await
    }

    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        rate_limits::delete(&self.root, id).await
    }

    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
        quotas::get(&self.root, scope, scope_id).await
    }

    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota> {
        let _guard = self.write.lock().await;
        quotas::upsert(&self.root, input).await
    }

    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool> {
        let _guard = self.write.lock().await;
        quotas::delete(&self.root, id).await
    }

    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        let _guard = self.write.lock().await;
        quotas::add_cost(&self.root, scope, scope_id, delta).await
    }

    async fn append_usage(&self, input: UsageInput) -> anyhow::Result<Option<Usage>> {
        let _guard = self.write.lock().await;
        usages::append(&self.root, input).await
    }

    async fn list_usages(&self, limit: u64) -> anyhow::Result<Vec<Usage>> {
        usages::list(&self.root, limit).await
    }

    async fn query_usages(&self, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
        usages::query(&self.root, q).await
    }

    async fn add_usage_rollup(&self, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
        let _guard = self.write.lock().await;
        usage_rollups::add(&self.root, input).await
    }

    async fn list_usage_rollups(
        &self,
        granularity: &str,
        from: i64,
        to: i64,
        user_id: Option<i64>,
    ) -> anyhow::Result<Vec<UsageRollup>> {
        usage_rollups::list(&self.root, granularity, from, to, user_id).await
    }

    async fn metrics_aggregate(
        &self,
    ) -> anyhow::Result<crate::store::persistence::metrics::MetricsAggregate> {
        super::metrics::aggregate(&self.root).await
    }

    async fn append_downstream_request(
        &self,
        input: DownstreamRequestInput,
    ) -> anyhow::Result<DownstreamRequest> {
        let _guard = self.write.lock().await;
        downstream_requests::append(&self.root, input).await
    }

    async fn list_downstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        downstream_requests::list(&self.root, request_id).await
    }

    async fn list_recent_downstream_requests(
        &self,
        limit: u64,
        before_id: Option<i64>,
    ) -> anyhow::Result<Vec<DownstreamRequest>> {
        downstream_requests::list_recent(&self.root, limit, before_id).await
    }

    async fn update_downstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        let _guard = self.write.lock().await;
        downstream_requests::update_response_body(&self.root, request_id, response_body).await
    }

    async fn append_upstream_request(
        &self,
        input: UpstreamRequestInput,
    ) -> anyhow::Result<UpstreamRequest> {
        let _guard = self.write.lock().await;
        upstream_requests::append(&self.root, input).await
    }

    async fn list_upstream_requests(
        &self,
        request_id: &str,
    ) -> anyhow::Result<Vec<UpstreamRequest>> {
        upstream_requests::list(&self.root, request_id).await
    }

    async fn update_upstream_response(
        &self,
        request_id: &str,
        response_body: Option<String>,
    ) -> anyhow::Result<()> {
        let _guard = self.write.lock().await;
        upstream_requests::update_response_body(&self.root, request_id, response_body).await
    }

    async fn purge_before(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let _guard = self.write.lock().await;
        let mut removed = 0u64;
        removed += usages::purge_before(&self.root, cutoff_ts).await?;
        removed += downstream_requests::purge_before(&self.root, cutoff_ts).await?;
        removed += upstream_requests::purge_before(&self.root, cutoff_ts).await?;
        Ok(removed)
    }

    async fn append_audit_log(&self, input: AuditLogInput) -> anyhow::Result<AuditLog> {
        let _guard = self.write.lock().await;
        audit_logs::append(&self.root, input).await
    }

    async fn list_audit_logs(&self, limit: u64) -> anyhow::Result<Vec<AuditLog>> {
        audit_logs::list(&self.root, limit).await
    }

    async fn list_instance_settings(&self) -> anyhow::Result<Vec<InstanceSettings>> {
        instance_settings::list(&self.root).await
    }

    async fn get_instance_settings(
        &self,
        instance_name: &str,
    ) -> anyhow::Result<Option<InstanceSettings>> {
        instance_settings::get(&self.root, instance_name).await
    }

    async fn upsert_instance_settings(
        &self,
        input: InstanceSettingsInput,
    ) -> anyhow::Result<InstanceSettings> {
        let _guard = self.write.lock().await;
        instance_settings::upsert(&self.root, input).await
    }

    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
        tokenizers::list(&self.root).await
    }

    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
        tokenizers::get(&self.root, name).await
    }

    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        tokenizers::put(&self.root, name, bytes).await
    }
}
