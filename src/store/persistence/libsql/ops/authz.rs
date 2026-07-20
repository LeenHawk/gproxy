use crate::store::persistence::records::{
    Quota, QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};
use crate::store::persistence::traits::AuthzPersistence;

use super::super::{LibsqlPersistence, authz};

#[async_trait::async_trait(?Send)]
impl AuthzPersistence for LibsqlPersistence {
    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>> {
        authz::route_permissions::list(&self.client, scope, scope_id).await
    }
    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission> {
        authz::route_permissions::upsert(&self.client, input).await
    }
    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool> {
        authz::route_permissions::delete(&self.client, id).await
    }
    async fn list_rate_limits(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RateLimit>> {
        authz::rate_limits::list(&self.client, scope, scope_id).await
    }
    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit> {
        authz::rate_limits::upsert(&self.client, input).await
    }
    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool> {
        authz::rate_limits::delete(&self.client, id).await
    }
    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
        authz::quotas::get(&self.client, scope, scope_id).await
    }
    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota> {
        authz::quotas::upsert(&self.client, input).await
    }
    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool> {
        authz::quotas::delete(&self.client, id).await
    }
    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        authz::quotas::add_cost(&self.client, scope, scope_id, delta).await
    }
}
