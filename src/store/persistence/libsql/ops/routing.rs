use crate::store::persistence::records::{
    Alias, AliasInput, ProviderRuleSet, ProviderRuleSetInput, Route, RouteInput, RouteMember,
    RouteMemberInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
};
use crate::store::persistence::traits::RoutingPersistence;

use super::super::{LibsqlPersistence, routing, transform};

#[async_trait::async_trait(?Send)]
impl RoutingPersistence for LibsqlPersistence {
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

    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
        transform::routing_rules::list(&self.client, provider_id).await
    }
    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>> {
        transform::routing_rules::get(&self.client, id).await
    }
    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
        transform::routing_rules::upsert(&self.client, input).await
    }
    async fn upsert_routing_rules_batch(
        &self,
        inputs: Vec<RoutingRuleInput>,
    ) -> anyhow::Result<()> {
        transform::routing_rules::upsert_batch(&self.client, &inputs).await
    }
    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool> {
        transform::routing_rules::delete(&self.client, id).await
    }

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
}
