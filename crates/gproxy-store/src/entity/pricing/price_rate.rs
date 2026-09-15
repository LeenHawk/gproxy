//! Base and dimension-conditional rates, including tokens, tools and media.
//! Multiple rows per metric allow distinct image sizes/qualities or tool names.
//! Select the first matching conditional row by (priority, id), otherwise the
//! first unconditional row. A selected conditional rate replaces the base rate.

use super::price_unit::PriceUnit;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "price_rates")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub price_rule_id: String,
    /// Built-in keys are in `pricing::metric`; custom usage metrics remain valid.
    pub metric: String,
    pub unit: PriceUnit,
    /// Positive denominator: e.g. 1_000_000 tokens, 1 image, or 60 seconds.
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub unit_quantity: Decimal,
    /// Nonnegative price for unit_quantity units, in the parent rule's currency.
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub value: Decimal,
    /// None is the fallback rate; otherwise a nonempty object of dimension
    /// names to scalar values. All conditions must match, e.g. size + quality.
    pub conditions: Option<Json>,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    #[sea_orm(belongs_to, from = "price_rule_id", to = "id", on_delete = "Cascade")]
    pub rule: BelongsTo<super::price_rule::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
