use crate::store::persistence::records::{
    Alias, AliasInput, ProviderRuleSet, ProviderRuleSetInput, Route, RouteInput, RouteMember,
    RouteMemberInput, RoutingRule, RoutingRuleInput, Rule, RuleInput, RuleSet, RuleSetInput,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait RoutingPersistence {
    async fn list_routes(&self) -> anyhow::Result<Vec<Route>>;
    async fn get_route(&self, id: i64) -> anyhow::Result<Option<Route>>;
    async fn get_route_by_name(&self, name: &str) -> anyhow::Result<Option<Route>>;
    async fn upsert_route(&self, input: RouteInput) -> anyhow::Result<Route>;
    async fn delete_route(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_route_members(&self, route_id: i64) -> anyhow::Result<Vec<RouteMember>>;
    async fn upsert_route_member(&self, input: RouteMemberInput) -> anyhow::Result<RouteMember>;
    async fn delete_route_member(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_aliases(&self) -> anyhow::Result<Vec<Alias>>;
    async fn get_alias_by_name(&self, alias: &str) -> anyhow::Result<Option<Alias>>;
    async fn upsert_alias(&self, input: AliasInput) -> anyhow::Result<Alias>;
    async fn delete_alias(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_routing_rules(&self, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>>;
    async fn get_routing_rule(&self, id: i64) -> anyhow::Result<Option<RoutingRule>>;
    async fn upsert_routing_rule(&self, input: RoutingRuleInput) -> anyhow::Result<RoutingRule>;
    async fn upsert_routing_rules_batch(
        &self,
        inputs: Vec<RoutingRuleInput>,
    ) -> anyhow::Result<()> {
        for input in inputs {
            self.upsert_routing_rule(input).await?;
        }
        Ok(())
    }
    async fn delete_routing_rule(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_rule_sets(&self) -> anyhow::Result<Vec<RuleSet>>;
    async fn get_rule_set(&self, id: i64) -> anyhow::Result<Option<RuleSet>>;
    async fn get_rule_set_by_name(&self, name: &str) -> anyhow::Result<Option<RuleSet>>;
    async fn upsert_rule_set(&self, input: RuleSetInput) -> anyhow::Result<RuleSet>;
    async fn delete_rule_set(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_rules(&self, rule_set_id: i64) -> anyhow::Result<Vec<Rule>>;
    async fn get_rule(&self, id: i64) -> anyhow::Result<Option<Rule>>;
    async fn upsert_rule(&self, input: RuleInput) -> anyhow::Result<Rule>;
    async fn delete_rule(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_provider_rule_sets(
        &self,
        provider_id: i64,
    ) -> anyhow::Result<Vec<ProviderRuleSet>>;
    async fn upsert_provider_rule_set(
        &self,
        input: ProviderRuleSetInput,
    ) -> anyhow::Result<ProviderRuleSet>;
    async fn delete_provider_rule_set(&self, id: i64) -> anyhow::Result<bool>;
}
