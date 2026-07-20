use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::records::{
    Quota, QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};
use crate::store::persistence::traits::AuthzPersistence;

#[async_trait]
impl AuthzPersistence for DbPersistence {
    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>> {
        ops::authz::route_permissions::list(&self.conn, scope, scope_id).await
    }
    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission> {
        ops::authz::route_permissions::upsert(&self.conn, input).await
    }
    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::route_permissions::delete(&self.conn, id).await
    }
    async fn list_rate_limits(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RateLimit>> {
        ops::authz::rate_limits::list(&self.conn, scope, scope_id).await
    }
    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit> {
        ops::authz::rate_limits::upsert(&self.conn, input).await
    }
    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::rate_limits::delete(&self.conn, id).await
    }
    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
        ops::authz::quotas::get(&self.conn, scope, scope_id).await
    }
    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota> {
        ops::authz::quotas::upsert(&self.conn, input).await
    }
    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool> {
        ops::authz::quotas::delete(&self.conn, id).await
    }
    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()> {
        ops::authz::quotas::add_cost(&self.conn, scope, scope_id, delta).await
    }
}
