use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::records::{
    Credential, CredentialInput, CredentialModelStatus, CredentialModelStatusInput,
    CredentialStatus, CredentialStatusInput, PriceRule, PriceRuleInput, Provider, ProviderInput,
    ProviderModel, ProviderModelInput,
};
use crate::store::persistence::traits::ProviderPersistence;

#[async_trait]
impl ProviderPersistence for DbPersistence {
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
    async fn list_credential_model_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialModelStatus>> {
        ops::provider::credential_model_statuses::list(&self.conn, credential_id).await
    }
    async fn list_all_credential_model_statuses(
        &self,
    ) -> anyhow::Result<Vec<CredentialModelStatus>> {
        ops::provider::credential_model_statuses::list_all(&self.conn).await
    }
    async fn upsert_credential_model_status(
        &self,
        input: CredentialModelStatusInput,
    ) -> anyhow::Result<CredentialModelStatus> {
        ops::provider::credential_model_statuses::upsert(&self.conn, input).await
    }
    async fn delete_credential_model_status(&self, id: i64) -> anyhow::Result<bool> {
        ops::provider::credential_model_statuses::delete(&self.conn, id).await
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

    async fn list_price_rules(&self) -> anyhow::Result<Vec<PriceRule>> {
        ops::pricing::price_rules::list(&self.conn).await
    }
    async fn upsert_price_rule(&self, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
        ops::pricing::price_rules::upsert(&self.conn, input).await
    }
    async fn delete_price_rule(&self, id: i64) -> anyhow::Result<bool> {
        ops::pricing::price_rules::delete(&self.conn, id).await
    }
}
