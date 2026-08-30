use crate::{Store, StoreError};

impl Store {
    pub async fn delete_provider(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("providers", id)?)
            .await
    }

    pub async fn delete_credential(&self, id: i64) -> Result<bool, StoreError> {
        let deleted = self
            .delete(crate::query::delete_by_id("credentials", id)?)
            .await?;
        if deleted {
            self.clear_credential_health(id).await?;
        }
        Ok(deleted)
    }

    pub async fn delete_route(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("routes", id)?).await
    }

    pub async fn delete_route_member(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("route_members", id)?)
            .await
    }

    pub async fn delete_alias(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("aliases", id)?)
            .await
    }

    pub async fn delete_exposed_model(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("exposed_models", id)?)
            .await
    }

    pub async fn delete_provider_model(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("provider_models", id)?)
            .await
    }

    pub async fn delete_organization(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("organizations", id)?)
            .await
    }

    pub async fn delete_team(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("teams", id)?).await
    }

    pub async fn delete_user(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("users", id)?).await
    }

    pub async fn delete_user_key(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("user_keys", id)?)
            .await
    }

    pub async fn delete_price_rule(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(crate::query::delete_by_id("price_rules", id)?)
            .await
    }
}
