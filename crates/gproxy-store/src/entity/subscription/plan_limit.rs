//! USD allowance per subscriber, copied to Quota(metric = cost, unit = USD)
//! on issuance. Later changes must not silently reset/resize existing windows.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_plan_limits")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "plan_window")]
    pub plan_id: String,
    /// Stable client window key, e.g. primary, secondary or seven_day_sonnet.
    #[sea_orm(unique_key = "plan_window")]
    pub window_key: String,
    /// Allowance in USD. Token/media/tool usage is priced in USD
    /// before settlement; upstream percentages are not dollar balances.
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub limit: Decimal,
    /// Same reset contract as Quota: total, fixed, day, week or month.
    pub period: String,
    /// Required positive duration for fixed windows; unused for calendar/total.
    pub period_seconds: Option<i64>,
    pub model_pattern: Option<String>,
    #[sea_orm(belongs_to, from = "plan_id", to = "id", on_delete = "Cascade")]
    pub plan: BelongsTo<super::plan::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
