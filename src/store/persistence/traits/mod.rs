//! Capability groups composing the public persistence backend contract.

mod authz;
mod identity;
mod provider;
mod routing;
mod settings;
mod usage;

pub use authz::AuthzPersistence;
pub use identity::IdentityPersistence;
pub use provider::ProviderPersistence;
pub use routing::RoutingPersistence;
pub use settings::SettingsPersistence;
pub use usage::UsagePersistence;

use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::metrics::MetricsAggregate;
use crate::store::persistence::records::*;
use crate::store::persistence::{LogQuery, UsageQuery};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait CorePersistence: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn health(&self) -> anyhow::Result<()>;
}

macro_rules! persistence_backend {
    ($( $cap:ident::$cap_method:ident => $method:ident($($arg:ident: $ty:ty),*) -> $ret:ty; )*) => {
        /// Durable storage abstraction.
        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        pub trait PersistenceBackend: Send + Sync {
            fn kind(&self) -> &'static str;
            async fn health(&self) -> anyhow::Result<()>;
            $(async fn $method(&self, $($arg: $ty),*) -> $ret;)*

            async fn upsert_routing_rules_batch(
                &self,
                inputs: Vec<RoutingRuleInput>,
            ) -> anyhow::Result<()> {
                for input in inputs {
                    self.upsert_routing_rule(input).await?;
                }
                Ok(())
            }

            async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
                Ok(vec![])
            }

            async fn get_tokenizer_vocab(&self, _name: &str) -> anyhow::Result<Option<Vec<u8>>> {
                Ok(None)
            }

            async fn put_tokenizer_vocab(&self, _name: &str, _bytes: &[u8]) -> anyhow::Result<()> {
                anyhow::bail!("tokenizer vocab storage unsupported by this backend")
            }
        }

        #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
        #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
        impl<T> PersistenceBackend for T
        where
            T: CorePersistence
                + ProviderPersistence
                + RoutingPersistence
                + IdentityPersistence
                + AuthzPersistence
                + UsagePersistence
                + SettingsPersistence,
        {
            fn kind(&self) -> &'static str {
                CorePersistence::kind(self)
            }

            async fn health(&self) -> anyhow::Result<()> {
                CorePersistence::health(self).await
            }

            async fn upsert_routing_rules_batch(
                &self,
                inputs: Vec<RoutingRuleInput>,
            ) -> anyhow::Result<()> {
                RoutingPersistence::upsert_routing_rules_batch(self, inputs).await
            }

            async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
                SettingsPersistence::list_tokenizer_vocabs(self).await
            }

            async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
                SettingsPersistence::get_tokenizer_vocab(self, name).await
            }

            async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
                SettingsPersistence::put_tokenizer_vocab(self, name, bytes).await
            }

            $(async fn $method(&self, $($arg: $ty),*) -> $ret {
                $cap::$cap_method(self, $($arg),*).await
            })*
        }
    };
}

persistence_backend! {
    ProviderPersistence::list_providers => list_providers() -> anyhow::Result<Vec<Provider>>;
    ProviderPersistence::get_provider => get_provider(id: i64) -> anyhow::Result<Option<Provider>>;
    ProviderPersistence::get_provider_by_name => get_provider_by_name(name: &str) -> anyhow::Result<Option<Provider>>;
    ProviderPersistence::upsert_provider => upsert_provider(input: ProviderInput) -> anyhow::Result<Provider>;
    ProviderPersistence::delete_provider => delete_provider(id: i64) -> anyhow::Result<bool>;
    ProviderPersistence::list_credentials => list_credentials(provider_id: i64) -> anyhow::Result<Vec<Credential>>;
    ProviderPersistence::get_credential => get_credential(id: i64) -> anyhow::Result<Option<Credential>>;
    ProviderPersistence::upsert_credential => upsert_credential(input: CredentialInput) -> anyhow::Result<Credential>;
    ProviderPersistence::update_credential_secret_if_current => update_credential_secret_if_current(id: i64, provider_id: i64, expected_updated_at: i64, secret_json: serde_json::Value) -> anyhow::Result<bool>;
    ProviderPersistence::delete_credential => delete_credential(id: i64) -> anyhow::Result<bool>;
    ProviderPersistence::list_credential_statuses => list_credential_statuses(credential_id: i64) -> anyhow::Result<Vec<CredentialStatus>>;
    ProviderPersistence::list_all_credential_statuses => list_all_credential_statuses() -> anyhow::Result<Vec<CredentialStatus>>;
    ProviderPersistence::upsert_credential_status => upsert_credential_status(input: CredentialStatusInput) -> anyhow::Result<CredentialStatus>;
    ProviderPersistence::delete_credential_status => delete_credential_status(id: i64) -> anyhow::Result<bool>;
    ProviderPersistence::list_credential_model_statuses => list_credential_model_statuses(credential_id: i64) -> anyhow::Result<Vec<CredentialModelStatus>>;
    ProviderPersistence::list_all_credential_model_statuses => list_all_credential_model_statuses() -> anyhow::Result<Vec<CredentialModelStatus>>;
    ProviderPersistence::upsert_credential_model_status => upsert_credential_model_status(input: CredentialModelStatusInput) -> anyhow::Result<CredentialModelStatus>;
    ProviderPersistence::delete_credential_model_status => delete_credential_model_status(id: i64) -> anyhow::Result<bool>;
    ProviderPersistence::list_provider_models => list_provider_models(provider_id: i64) -> anyhow::Result<Vec<ProviderModel>>;
    ProviderPersistence::upsert_provider_model => upsert_provider_model(input: ProviderModelInput) -> anyhow::Result<ProviderModel>;
    ProviderPersistence::delete_provider_model => delete_provider_model(id: i64) -> anyhow::Result<bool>;
    ProviderPersistence::list_price_rules => list_price_rules() -> anyhow::Result<Vec<PriceRule>>;
    ProviderPersistence::upsert_price_rule => upsert_price_rule(input: PriceRuleInput) -> anyhow::Result<PriceRule>;
    ProviderPersistence::delete_price_rule => delete_price_rule(id: i64) -> anyhow::Result<bool>;

    RoutingPersistence::list_routes => list_routes() -> anyhow::Result<Vec<Route>>;
    RoutingPersistence::get_route => get_route(id: i64) -> anyhow::Result<Option<Route>>;
    RoutingPersistence::get_route_by_name => get_route_by_name(name: &str) -> anyhow::Result<Option<Route>>;
    RoutingPersistence::upsert_route => upsert_route(input: RouteInput) -> anyhow::Result<Route>;
    RoutingPersistence::delete_route => delete_route(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_route_members => list_route_members(route_id: i64) -> anyhow::Result<Vec<RouteMember>>;
    RoutingPersistence::upsert_route_member => upsert_route_member(input: RouteMemberInput) -> anyhow::Result<RouteMember>;
    RoutingPersistence::delete_route_member => delete_route_member(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_aliases => list_aliases() -> anyhow::Result<Vec<Alias>>;
    RoutingPersistence::get_alias_by_name => get_alias_by_name(alias: &str) -> anyhow::Result<Option<Alias>>;
    RoutingPersistence::upsert_alias => upsert_alias(input: AliasInput) -> anyhow::Result<Alias>;
    RoutingPersistence::delete_alias => delete_alias(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_routing_rules => list_routing_rules(provider_id: i64) -> anyhow::Result<Vec<RoutingRule>>;
    RoutingPersistence::get_routing_rule => get_routing_rule(id: i64) -> anyhow::Result<Option<RoutingRule>>;
    RoutingPersistence::upsert_routing_rule => upsert_routing_rule(input: RoutingRuleInput) -> anyhow::Result<RoutingRule>;
    RoutingPersistence::delete_routing_rule => delete_routing_rule(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_rule_sets => list_rule_sets() -> anyhow::Result<Vec<RuleSet>>;
    RoutingPersistence::get_rule_set => get_rule_set(id: i64) -> anyhow::Result<Option<RuleSet>>;
    RoutingPersistence::get_rule_set_by_name => get_rule_set_by_name(name: &str) -> anyhow::Result<Option<RuleSet>>;
    RoutingPersistence::upsert_rule_set => upsert_rule_set(input: RuleSetInput) -> anyhow::Result<RuleSet>;
    RoutingPersistence::delete_rule_set => delete_rule_set(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_rules => list_rules(rule_set_id: i64) -> anyhow::Result<Vec<Rule>>;
    RoutingPersistence::get_rule => get_rule(id: i64) -> anyhow::Result<Option<Rule>>;
    RoutingPersistence::upsert_rule => upsert_rule(input: RuleInput) -> anyhow::Result<Rule>;
    RoutingPersistence::delete_rule => delete_rule(id: i64) -> anyhow::Result<bool>;
    RoutingPersistence::list_provider_rule_sets => list_provider_rule_sets(provider_id: i64) -> anyhow::Result<Vec<ProviderRuleSet>>;
    RoutingPersistence::upsert_provider_rule_set => upsert_provider_rule_set(input: ProviderRuleSetInput) -> anyhow::Result<ProviderRuleSet>;
    RoutingPersistence::delete_provider_rule_set => delete_provider_rule_set(id: i64) -> anyhow::Result<bool>;

    IdentityPersistence::list_orgs => list_orgs() -> anyhow::Result<Vec<Org>>;
    IdentityPersistence::get_org => get_org(id: i64) -> anyhow::Result<Option<Org>>;
    IdentityPersistence::get_org_by_name => get_org_by_name(name: &str) -> anyhow::Result<Option<Org>>;
    IdentityPersistence::upsert_org => upsert_org(input: OrgInput) -> anyhow::Result<Org>;
    IdentityPersistence::delete_org => delete_org(id: i64) -> anyhow::Result<bool>;
    IdentityPersistence::list_teams => list_teams(org_id: i64) -> anyhow::Result<Vec<Team>>;
    IdentityPersistence::get_team => get_team(id: i64) -> anyhow::Result<Option<Team>>;
    IdentityPersistence::upsert_team => upsert_team(input: TeamInput) -> anyhow::Result<Team>;
    IdentityPersistence::delete_team => delete_team(id: i64) -> anyhow::Result<bool>;
    IdentityPersistence::list_users => list_users() -> anyhow::Result<Vec<User>>;
    IdentityPersistence::get_user => get_user(id: i64) -> anyhow::Result<Option<User>>;
    IdentityPersistence::get_user_by_name => get_user_by_name(name: &str) -> anyhow::Result<Option<User>>;
    IdentityPersistence::upsert_user => upsert_user(input: UserInput) -> anyhow::Result<User>;
    IdentityPersistence::delete_user => delete_user(id: i64) -> anyhow::Result<bool>;
    IdentityPersistence::list_user_keys => list_user_keys(user_id: i64) -> anyhow::Result<Vec<UserKey>>;
    IdentityPersistence::get_user_key => get_user_key(id: i64) -> anyhow::Result<Option<UserKey>>;
    IdentityPersistence::find_user_key_by_digest => find_user_key_by_digest(digest: &str) -> anyhow::Result<Option<UserKey>>;
    IdentityPersistence::upsert_user_key => upsert_user_key(input: UserKeyInput) -> anyhow::Result<UserKey>;
    IdentityPersistence::delete_user_key => delete_user_key(id: i64) -> anyhow::Result<bool>;

    AuthzPersistence::list_route_permissions => list_route_permissions(scope: Scope, scope_id: i64) -> anyhow::Result<Vec<RoutePermission>>;
    AuthzPersistence::upsert_route_permission => upsert_route_permission(input: RoutePermissionInput) -> anyhow::Result<RoutePermission>;
    AuthzPersistence::delete_route_permission => delete_route_permission(id: i64) -> anyhow::Result<bool>;
    AuthzPersistence::list_rate_limits => list_rate_limits(scope: Scope, scope_id: i64) -> anyhow::Result<Vec<RateLimit>>;
    AuthzPersistence::upsert_rate_limit => upsert_rate_limit(input: RateLimitInput) -> anyhow::Result<RateLimit>;
    AuthzPersistence::delete_rate_limit => delete_rate_limit(id: i64) -> anyhow::Result<bool>;
    AuthzPersistence::get_quota => get_quota(scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>>;
    AuthzPersistence::upsert_quota => upsert_quota(input: QuotaInput) -> anyhow::Result<Quota>;
    AuthzPersistence::delete_quota => delete_quota(id: i64) -> anyhow::Result<bool>;
    AuthzPersistence::add_quota_cost => add_quota_cost(scope: Scope, scope_id: i64, delta: rust_decimal::Decimal) -> anyhow::Result<()>;

    UsagePersistence::append_usage => append_usage(input: UsageInput) -> anyhow::Result<Option<Usage>>;
    UsagePersistence::list_usages => list_usages(limit: u64) -> anyhow::Result<Vec<Usage>>;
    UsagePersistence::query_usages => query_usages(q: &UsageQuery) -> anyhow::Result<Vec<Usage>>;
    UsagePersistence::summarize_usages => summarize_usages(q: &UsageQuery) -> anyhow::Result<UsageSummary>;
    UsagePersistence::add_usage_rollup => add_usage_rollup(input: UsageRollupInput) -> anyhow::Result<UsageRollup>;
    UsagePersistence::list_usage_rollups => list_usage_rollups(granularity: &str, from: i64, to: i64, user_id: Option<i64>) -> anyhow::Result<Vec<UsageRollup>>;
    UsagePersistence::metrics_aggregate => metrics_aggregate() -> anyhow::Result<MetricsAggregate>;
    UsagePersistence::append_downstream_request => append_downstream_request(input: DownstreamRequestInput) -> anyhow::Result<DownstreamRequest>;
    UsagePersistence::list_downstream_requests => list_downstream_requests(request_id: &str) -> anyhow::Result<Vec<DownstreamRequest>>;
    UsagePersistence::query_downstream_requests => query_downstream_requests(q: &LogQuery) -> anyhow::Result<Vec<DownstreamRequest>>;
    UsagePersistence::update_downstream_response => update_downstream_response(request_id: &str, response_body: Option<String>) -> anyhow::Result<()>;
    UsagePersistence::append_upstream_request => append_upstream_request(input: UpstreamRequestInput) -> anyhow::Result<UpstreamRequest>;
    UsagePersistence::list_upstream_requests => list_upstream_requests(request_id: &str) -> anyhow::Result<Vec<UpstreamRequest>>;
    UsagePersistence::update_upstream_response => update_upstream_response(request_id: &str, response_body: Option<String>) -> anyhow::Result<()>;
    UsagePersistence::delete_usage => delete_usage(id: i64) -> anyhow::Result<bool>;
    UsagePersistence::set_enabled => set_enabled(entity: AdminEntity, id: i64, enabled: bool) -> anyhow::Result<bool>;
    UsagePersistence::purge_before => purge_before(cutoff_ts: i64) -> anyhow::Result<u64>;
    UsagePersistence::append_audit_log => append_audit_log(input: AuditLogInput) -> anyhow::Result<AuditLog>;
    UsagePersistence::list_audit_logs => list_audit_logs(limit: u64) -> anyhow::Result<Vec<AuditLog>>;

    SettingsPersistence::list_instance_settings => list_instance_settings() -> anyhow::Result<Vec<InstanceSettings>>;
    SettingsPersistence::get_instance_settings => get_instance_settings(instance_name: &str) -> anyhow::Result<Option<InstanceSettings>>;
    SettingsPersistence::upsert_instance_settings => upsert_instance_settings(input: InstanceSettingsInput) -> anyhow::Result<InstanceSettings>;
}
