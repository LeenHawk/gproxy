use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::records::{
    Alias, AliasInput, ProviderRuleSet, ProviderRuleSetInput, Route, RouteInput, RouteMember,
    RouteMemberInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
};
use crate::store::persistence::traits::RoutingPersistence;

#[async_trait]
impl RoutingPersistence for DbPersistence {
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
}
