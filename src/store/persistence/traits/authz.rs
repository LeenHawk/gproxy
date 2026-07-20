use crate::store::persistence::records::{
    Quota, QuotaInput, RateLimit, RateLimitInput, RoutePermission, RoutePermissionInput, Scope,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AuthzPersistence {
    async fn list_route_permissions(
        &self,
        scope: Scope,
        scope_id: i64,
    ) -> anyhow::Result<Vec<RoutePermission>>;
    async fn upsert_route_permission(
        &self,
        input: RoutePermissionInput,
    ) -> anyhow::Result<RoutePermission>;
    async fn delete_route_permission(&self, id: i64) -> anyhow::Result<bool>;
    async fn list_rate_limits(&self, scope: Scope, scope_id: i64)
    -> anyhow::Result<Vec<RateLimit>>;
    async fn upsert_rate_limit(&self, input: RateLimitInput) -> anyhow::Result<RateLimit>;
    async fn delete_rate_limit(&self, id: i64) -> anyhow::Result<bool>;
    async fn get_quota(&self, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>>;
    async fn upsert_quota(&self, input: QuotaInput) -> anyhow::Result<Quota>;
    async fn delete_quota(&self, id: i64) -> anyhow::Result<bool>;
    async fn add_quota_cost(
        &self,
        scope: Scope,
        scope_id: i64,
        delta: rust_decimal::Decimal,
    ) -> anyhow::Result<()>;
}
