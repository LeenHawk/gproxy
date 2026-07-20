use crate::store::persistence::records::{
    Credential, CredentialInput, CredentialModelStatus, CredentialModelStatusInput,
    CredentialStatus, CredentialStatusInput, PriceRule, PriceRuleInput, Provider, ProviderInput,
    ProviderModel, ProviderModelInput,
};
use crate::store::persistence::traits::ProviderPersistence;

use super::super::{LibsqlPersistence, pricing, provider};

#[async_trait::async_trait(?Send)]
impl ProviderPersistence for LibsqlPersistence {
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
    async fn list_credential_model_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialModelStatus>> {
        provider::credential_model_statuses::list(&self.client, credential_id).await
    }
    async fn list_all_credential_model_statuses(
        &self,
    ) -> anyhow::Result<Vec<CredentialModelStatus>> {
        provider::credential_model_statuses::list_all(&self.client).await
    }
    async fn upsert_credential_model_status(
        &self,
        input: CredentialModelStatusInput,
    ) -> anyhow::Result<CredentialModelStatus> {
        provider::credential_model_statuses::upsert(&self.client, input).await
    }
    async fn delete_credential_model_status(&self, id: i64) -> anyhow::Result<bool> {
        provider::credential_model_statuses::delete(&self.client, id).await
    }

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

    async fn list_price_rules(&self) -> anyhow::Result<Vec<PriceRule>> {
        pricing::price_rules::list(&self.client).await
    }
    async fn upsert_price_rule(&self, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
        pricing::price_rules::upsert(&self.client, input).await
    }
    async fn delete_price_rule(&self, id: i64) -> anyhow::Result<bool> {
        pricing::price_rules::delete(&self.client, id).await
    }
}
