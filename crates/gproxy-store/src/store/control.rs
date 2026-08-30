use crate::query::control;
use crate::records::{
    AliasInput, CredentialAdminRecord, CredentialInput, ExposedModelInput, PriceRateInput,
    PriceRuleInput, ProviderInput, ProviderModelInput, RouteInput, RouteMemberInput, SettingInput,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn admin_credentials(&self) -> Result<Vec<CredentialAdminRecord>, StoreError> {
        self.backend()
            .execute(control::select_admin_credentials()?)
            .await?
            .rows
            .into_iter()
            .map(|row| {
                Ok(CredentialAdminRecord {
                    id: row.i64("id")?,
                    provider_id: row.i64("provider_id")?,
                    label: row.optional_text("label")?.map(str::to_owned),
                    kind: row.text("kind")?.to_owned(),
                    version: u64::try_from(row.i64("version")?).map_err(|error| {
                        StoreError::InvalidData {
                            field: "credential version",
                            message: error.to_string(),
                        }
                    })?,
                    enabled: row.i64("enabled")? != 0,
                    weight: u32::try_from(row.i64("weight")?).map_err(|error| {
                        StoreError::InvalidData {
                            field: "credential weight",
                            message: error.to_string(),
                        }
                    })?,
                    rpm_limit: row
                        .optional_i64("rpm_limit")?
                        .map(|value| {
                            u32::try_from(value).map_err(|error| StoreError::InvalidData {
                                field: "credential rpm_limit",
                                message: error.to_string(),
                            })
                        })
                        .transpose()?,
                    tpm_limit: row
                        .optional_i64("tpm_limit")?
                        .map(|value| {
                            u64::try_from(value).map_err(|error| StoreError::InvalidData {
                                field: "credential tpm_limit",
                                message: error.to_string(),
                            })
                        })
                        .transpose()?,
                    proxy_url: row.optional_text("proxy_url")?.map(str::to_owned),
                    tls_fingerprint: row
                        .optional_text("tls_fingerprint")?
                        .map(|value| {
                            serde_json::from_str(value).map_err(|error| StoreError::InvalidData {
                                field: "credential tls_fingerprint",
                                message: error.to_string(),
                            })
                        })
                        .transpose()?,
                })
            })
            .collect()
    }

    pub async fn insert_provider(&self, input: &ProviderInput) -> Result<i64, StoreError> {
        self.insert(control::insert_provider(input)?).await
    }

    pub async fn update_provider(
        &self,
        id: i64,
        input: &ProviderInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_provider(id, input)?).await
    }

    pub async fn insert_credential(&self, input: &CredentialInput) -> Result<i64, StoreError> {
        self.insert(control::insert_credential(input)?).await
    }

    pub async fn update_credential(
        &self,
        id: i64,
        input: &crate::records::CredentialUpdateInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_credential(id, input)?).await
    }

    pub async fn insert_route(&self, input: &RouteInput) -> Result<i64, StoreError> {
        self.insert(control::insert_route(input)?).await
    }

    pub async fn update_route(&self, id: i64, input: &RouteInput) -> Result<bool, StoreError> {
        self.update(control::update_route(id, input)?).await
    }

    pub async fn insert_route_member(&self, input: &RouteMemberInput) -> Result<i64, StoreError> {
        self.insert(control::insert_route_member(input)?).await
    }

    pub async fn update_route_member(
        &self,
        id: i64,
        input: &RouteMemberInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_route_member(id, input)?).await
    }

    pub async fn insert_alias(&self, input: &AliasInput) -> Result<i64, StoreError> {
        self.insert(control::insert_alias(input)?).await
    }

    pub async fn update_alias(&self, id: i64, input: &AliasInput) -> Result<bool, StoreError> {
        self.update(control::update_alias(id, input)?).await
    }

    pub async fn insert_exposed_model(&self, input: &ExposedModelInput) -> Result<i64, StoreError> {
        self.insert(control::insert_exposed_model(input)?).await
    }

    pub async fn update_exposed_model(
        &self,
        id: i64,
        input: &ExposedModelInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_exposed_model(id, input)?).await
    }

    pub async fn insert_provider_model(
        &self,
        input: &ProviderModelInput,
    ) -> Result<i64, StoreError> {
        self.insert(control::insert_provider_model(input)?).await
    }

    pub async fn update_provider_model(
        &self,
        id: i64,
        input: &ProviderModelInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_provider_model(id, input)?)
            .await
    }

    pub async fn insert_price_rule(&self, input: &PriceRuleInput) -> Result<i64, StoreError> {
        self.insert(control::insert_price_rule(input)?).await
    }

    pub async fn update_price_rule(
        &self,
        id: i64,
        input: &PriceRuleInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_price_rule(id, input)?).await
    }

    pub async fn insert_price_rate(&self, input: &PriceRateInput) -> Result<i64, StoreError> {
        self.insert(control::insert_price_rate(input)?).await
    }

    pub async fn update_price_rate(
        &self,
        id: i64,
        input: &PriceRateInput,
    ) -> Result<bool, StoreError> {
        self.update(control::update_price_rate(id, input)?).await
    }

    pub async fn delete_price_rate(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(control::delete_price_rate(id)?).await
    }

    pub async fn set_setting(&self, input: &SettingInput) -> Result<(), StoreError> {
        self.backend()
            .execute(control::insert_setting(input)?)
            .await?;
        Ok(())
    }

    pub async fn set_settings(&self, inputs: &[SettingInput]) -> Result<(), StoreError> {
        let statements = inputs
            .iter()
            .map(control::insert_setting)
            .collect::<Result<Vec<_>, _>>()?;
        self.backend().batch(statements).await?;
        Ok(())
    }
}
