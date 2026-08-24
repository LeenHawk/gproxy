use crate::query::control;
use crate::records::{
    AliasInput, CredentialInput, ExposedModelInput, PriceRateInput, PriceRuleInput, ProviderInput,
    RouteInput, RouteMemberInput, SettingInput,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn insert_provider(&self, input: &ProviderInput) -> Result<i64, StoreError> {
        self.insert(control::insert_provider(input)?).await
    }

    pub async fn insert_credential(&self, input: &CredentialInput) -> Result<i64, StoreError> {
        self.insert(control::insert_credential(input)?).await
    }

    pub async fn insert_route(&self, input: &RouteInput) -> Result<i64, StoreError> {
        self.insert(control::insert_route(input)?).await
    }

    pub async fn insert_route_member(&self, input: &RouteMemberInput) -> Result<i64, StoreError> {
        self.insert(control::insert_route_member(input)?).await
    }

    pub async fn insert_alias(&self, input: &AliasInput) -> Result<i64, StoreError> {
        self.insert(control::insert_alias(input)?).await
    }

    pub async fn insert_exposed_model(&self, input: &ExposedModelInput) -> Result<i64, StoreError> {
        self.insert(control::insert_exposed_model(input)?).await
    }

    pub async fn insert_price_rule(&self, input: &PriceRuleInput) -> Result<i64, StoreError> {
        self.insert(control::insert_price_rule(input)?).await
    }

    pub async fn insert_price_rate(&self, input: &PriceRateInput) -> Result<i64, StoreError> {
        self.insert(control::insert_price_rate(input)?).await
    }

    pub async fn set_setting(&self, input: &SettingInput) -> Result<(), StoreError> {
        self.backend()
            .execute(control::insert_setting(input)?)
            .await?;
        Ok(())
    }
}
