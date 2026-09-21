//! A gateway-defined downstream plan backed by a capacity pool.
//! Client labels describe this virtual plan, not a purchased upstream entitlement.
//! Treat issued plans as immutable; publish another plan to change pool/terms.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_plans")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub pool_id: String,
    pub name: String,
    /// Optional client wire labels, validated/mapped by the corresponding adapter.
    pub codex_plan_type: Option<String>,
    pub claude_subscription_type: Option<String>,
    pub claude_rate_limit_tier: Option<String>,
    /// Controls new issuance; disabling a plan alone does not revoke issued subscriptions.
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "pool_id", to = "id", on_delete = "Cascade")]
    pub pool: BelongsTo<super::pool::Entity>,
    #[sea_orm(has_many)]
    pub limits: HasMany<super::plan_limit::Entity>,
    #[sea_orm(has_many)]
    pub subscriptions: HasMany<super::user_subscription::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
