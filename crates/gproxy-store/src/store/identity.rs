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

    pub async fn insert_team(&self, input: &TeamInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_team(input)?).await
    }

    pub async fn insert_user(&self, input: &UserInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_user(input)?).await
    }

    pub async fn insert_user_key(&self, input: &UserKeyInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_user_key(input)?).await
    }

    pub async fn insert_permission(&self, input: &PermissionInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_permission(input)?).await
    }

    pub async fn insert_rate_limit(&self, input: &RateLimitInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_rate_limit(input)?).await
    }

    pub async fn insert_quota(&self, input: &QuotaInput) -> Result<i64, StoreError> {
        self.insert(identity::insert_quota(input)?).await
    }
}
