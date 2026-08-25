use crate::query::identity;
use crate::records::{
    OrganizationInput, PermissionInput, QuotaInput, RateLimitInput, TeamInput, UserInput,
    UserKeyInput,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn insert_organization(&self, input: &OrganizationInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_organization(input)?).await
    }

    pub async fn update_organization(
        &self,
        id: i64,
        input: &OrganizationInput,
    ) -> Result<bool, StoreError> {
        self.update(identity::update_organization(id, input)?).await
    }

    pub async fn insert_team(&self, input: &TeamInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_team(input)?).await
    }

    pub async fn update_team(&self, id: i64, input: &TeamInput) -> Result<bool, StoreError> {
        self.update(identity::update_team(id, input)?).await
    }

    pub async fn insert_user(&self, input: &UserInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_user(input)?).await
    }

    pub async fn update_user(&self, id: i64, input: &UserInput) -> Result<bool, StoreError> {
        self.update(identity::update_user(id, input)?).await
    }

    pub async fn insert_user_key(&self, input: &UserKeyInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_user_key(input)?).await
    }

    pub async fn update_user_key(
        &self,
        id: i64,
        input: &crate::records::UserKeyUpdateInput,
    ) -> Result<bool, StoreError> {
        self.update(identity::update_user_key(id, input)?).await
    }

    pub async fn insert_permission(&self, input: &PermissionInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_permission(input)?).await
    }

    pub async fn update_permission(
        &self,
        id: i64,
        input: &PermissionInput,
    ) -> Result<bool, StoreError> {
        self.update(identity::update_permission(id, input)?).await
    }

    pub async fn delete_permission(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(identity::delete_permission(id)?).await
    }

    pub async fn insert_rate_limit(&self, input: &RateLimitInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_rate_limit(input)?).await
    }

    pub async fn update_rate_limit(
        &self,
        id: i64,
        input: &RateLimitInput,
    ) -> Result<bool, StoreError> {
        self.update(identity::update_rate_limit(id, input)?).await
    }

    pub async fn delete_rate_limit(&self, id: i64) -> Result<bool, StoreError> {
        self.delete(identity::delete_rate_limit(id)?).await
    }

    pub async fn insert_quota(&self, input: &QuotaInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_quota(input)?).await
    }

    pub async fn update_quota(&self, id: i64, input: &QuotaInput) -> Result<bool, StoreError> {
        self.update(identity::update_quota(id, input)?).await
    }

    pub async fn delete_quota(&self, id: i64) -> Result<bool, StoreError> {
        self.update(identity::delete_quota(id)?).await
    }
}
