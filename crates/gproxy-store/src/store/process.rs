use crate::query::control;
use crate::records::{ProviderRuleSetInput, RoutingRuleInput, RuleInput, RuleSetInput};
use crate::{Store, StoreError};

impl Store {
    pub async fn insert_routing_rule(&self, input: &RoutingRuleInput) -> Result<i64, StoreError> {
        self.insert(control::insert_routing_rule(input)?).await
    }
    pub async fn update_routing_rule(
        &self,
        id: i64,
        input: &RoutingRuleInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_routing_rule(id, input)?).await
    }
    pub async fn delete_routing_rule(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(control::delete_process("routing_rules", id)?)
            .await
    }

    pub async fn insert_rule_set(&self, input: &RuleSetInput) -> Result<i64, StoreError> {
        self.insert(control::insert_rule_set(input)?).await
    }
    pub async fn update_rule_set(&self, id: i64, input: &RuleSetInput) -> Result<bool, StoreError> {
        self.update(control::update_rule_set(id, input)?).await
    }
    pub async fn delete_rule_set(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(control::delete_process("rule_sets", id)?).await
    }

    pub async fn insert_rule(&self, input: &RuleInput) -> Result<i64, StoreError> {
        self.insert(control::insert_rule(input)?).await
    }
    pub async fn update_rule(&self, id: i64, input: &RuleInput) -> Result<bool, StoreError> {
        self.update(control::update_rule(id, input)?).await
    }
    pub async fn delete_rule(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(control::delete_process("rules", id)?).await
    }

    pub async fn insert_provider_rule_set(
        &self,
        input: &ProviderRuleSetInput,
    ) -> Result<i64, StoreError> {
        self.insert(control::insert_provider_rule_set(input)?).await
    }
    pub async fn update_provider_rule_set(
        &self,
        id: i64,
        input: &ProviderRuleSetInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_provider_rule_set(id, input)?)
            .await
    }
    pub async fn delete_provider_rule_set(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(control::delete_process("provider_rule_sets", id)?)
            .await
    }
}
