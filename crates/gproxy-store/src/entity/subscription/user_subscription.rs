//! One user's issued virtual subscription, shared by its API keys/OAuth sessions.
//! Allocation is stored in subscription-owned Quota rows; consumption reuses
//! QuotaWindow and QuotaSettlement. Renewal creates new windows, not new usage.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscriptions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    #[sea_orm(indexed)]
    pub plan_id: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    /// Anchor for fixed-duration allocation windows unless explicitly overridden.
    pub starts_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<crate::entity::identity::user::Entity>,
    /// Retain the plan while issued subscriptions refer to it; disable for retirement.
    #[sea_orm(belongs_to, from = "plan_id", to = "id", on_delete = "Restrict")]
    pub plan: BelongsTo<super::plan::Entity>,
    #[sea_orm(has_many)]
    pub quotas: HasMany<crate::entity::limits::quota::Entity>,
    #[sea_orm(has_many)]
    pub api_keys: HasMany<crate::entity::identity::api_key::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
