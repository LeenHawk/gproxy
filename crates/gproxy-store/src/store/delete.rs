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

    /// A route owns its members and the public names that map to it; the
    /// schema has no foreign keys across backends, so the rows go together
    /// here. A name left behind would keep the alias taken by nothing the
    /// console can show.
    pub async fn delete_route(&self, id: i64) -> Result<bool, StoreError> {
        let results = self
            .backend()
            .batch(vec![
                crate::query::delete_where("route_members", "route_id", id)?,
                crate::query::delete_where("exposed_models", "route_id", id)?,
                crate::query::delete_by_id("routes", id)?,
            ])
            .await?;
        Ok(results
            .last()
            .is_some_and(|result| result.affected_rows == 1))
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
        let current = self
            .control_snapshot()
            .await?
            .provider_models
            .into_iter()
            .find(|model| model.id == id);
        let Some(current) = current else {
            return Ok(false);
        };
        let mut statements = vec![crate::query::delete_by_id("provider_models", id)?];
        statements.extend(crate::query::control::delete_model_metadata(
            current.provider_id,
            &current.model_id,
        )?);
        Ok(self
            .backend()
            .batch(statements)
            .await?
            .first()
            .is_some_and(|result| result.affected_rows == 1))
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
