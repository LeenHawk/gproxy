use crate::store::persistence::records::{
    Credential, CredentialInput, CredentialStatus, CredentialStatusInput, PriceRule,
    PriceRuleInput, Provider, ProviderInput, ProviderModel, ProviderModelInput,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ProviderPersistence {
    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>>;
    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>>;
    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>>;
    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider>;
    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>>;
    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>>;
    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential>;
    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool>;
    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>>;
    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>>;
    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus>;
    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>>;
    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel>;
    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_price_rules(&self) -> anyhow::Result<Vec<PriceRule>>;
    async fn upsert_price_rule(&self, input: PriceRuleInput) -> anyhow::Result<PriceRule>;
    async fn delete_price_rule(&self, id: i64) -> anyhow::Result<bool>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ProviderPersistence for dyn super::PersistenceBackend + '_ {
    async fn list_providers(&self) -> anyhow::Result<Vec<Provider>> {
        super::PersistenceBackend::list_providers(self).await
    }
    async fn get_provider(&self, id: i64) -> anyhow::Result<Option<Provider>> {
        super::PersistenceBackend::get_provider(self, id).await
    }
    async fn get_provider_by_name(&self, name: &str) -> anyhow::Result<Option<Provider>> {
        super::PersistenceBackend::get_provider_by_name(self, name).await
    }
    async fn upsert_provider(&self, input: ProviderInput) -> anyhow::Result<Provider> {
        super::PersistenceBackend::upsert_provider(self, input).await
    }
    async fn delete_provider(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_provider(self, id).await
    }
    async fn list_credentials(&self, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
        super::PersistenceBackend::list_credentials(self, provider_id).await
    }
    async fn get_credential(&self, id: i64) -> anyhow::Result<Option<Credential>> {
        super::PersistenceBackend::get_credential(self, id).await
    }
    async fn upsert_credential(&self, input: CredentialInput) -> anyhow::Result<Credential> {
        super::PersistenceBackend::upsert_credential(self, input).await
    }
    async fn update_credential_secret_if_current(
        &self,
        id: i64,
        provider_id: i64,
        expected_updated_at: i64,
        secret_json: serde_json::Value,
    ) -> anyhow::Result<bool> {
        super::PersistenceBackend::update_credential_secret_if_current(
            self,
            id,
            provider_id,
            expected_updated_at,
            secret_json,
        )
        .await
    }
    async fn delete_credential(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_credential(self, id).await
    }
    async fn list_credential_statuses(
        &self,
        credential_id: i64,
    ) -> anyhow::Result<Vec<CredentialStatus>> {
        super::PersistenceBackend::list_credential_statuses(self, credential_id).await
    }
    async fn list_all_credential_statuses(&self) -> anyhow::Result<Vec<CredentialStatus>> {
        super::PersistenceBackend::list_all_credential_statuses(self).await
    }
    async fn upsert_credential_status(
        &self,
        input: CredentialStatusInput,
    ) -> anyhow::Result<CredentialStatus> {
        super::PersistenceBackend::upsert_credential_status(self, input).await
    }
    async fn delete_credential_status(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_credential_status(self, id).await
    }
    async fn list_provider_models(&self, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
        super::PersistenceBackend::list_provider_models(self, provider_id).await
    }
    async fn upsert_provider_model(
        &self,
        input: ProviderModelInput,
    ) -> anyhow::Result<ProviderModel> {
        super::PersistenceBackend::upsert_provider_model(self, input).await
    }
    async fn delete_provider_model(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_provider_model(self, id).await
    }
    async fn list_price_rules(&self) -> anyhow::Result<Vec<PriceRule>> {
        super::PersistenceBackend::list_price_rules(self).await
    }
    async fn upsert_price_rule(&self, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
        super::PersistenceBackend::upsert_price_rule(self, input).await
    }
    async fn delete_price_rule(&self, id: i64) -> anyhow::Result<bool> {
        super::PersistenceBackend::delete_price_rule(self, id).await
    }
}
