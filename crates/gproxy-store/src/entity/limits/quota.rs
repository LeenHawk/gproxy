//! Configured budgets, separate from historical consumption windows.
//! Exactly one owner: user, API key, subscription, or subscription pool.
//! Pool budgets measure actual upstream work; subscription budgets measure
//! allocated downstream consumption. They are not upstream quota observations.
//! Subscription allocations and their pool monetary budgets use metric = cost,
//! unit = USD. Other user/API-key limits may use other metrics/units. The future
//! write layer must enforce the subscription denomination contract.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quotas")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    #[sea_orm(indexed)]
    pub subscription_id: Option<String>,
    #[sea_orm(indexed)]
    pub pool_id: Option<String>,
    /// Stable window key used by subscription views, e.g. primary/secondary.
    pub window_key: String,
    pub metric: String,
    pub unit: String,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub limit_value: FixedDecimal,
    pub period: String,
    pub period_seconds: Option<i64>,
    /// Fixed-window anchor; subscription issuance defaults this to starts_at_ms.
    /// Calendar windows use UTC; total has no reset. Sliding windows are not implied.
    pub anchor_at_ms: Option<i64>,
    pub model_pattern: Option<String>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<crate::entity::identity::user::Entity>>,
    #[sea_orm(belongs_to, from = "api_key_id", to = "id", on_delete = "Cascade")]
    pub api_key: BelongsTo<Option<crate::entity::identity::api_key::Entity>>,
    #[sea_orm(belongs_to, from = "subscription_id", to = "id", on_delete = "Cascade")]
    pub subscription: BelongsTo<Option<crate::entity::subscription::user_subscription::Entity>>,
    #[sea_orm(belongs_to, from = "pool_id", to = "id", on_delete = "Cascade")]
    pub pool: BelongsTo<Option<crate::entity::subscription::pool::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
