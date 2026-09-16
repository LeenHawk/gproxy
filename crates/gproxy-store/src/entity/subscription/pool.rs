//! An upstream capacity pool backing one or more downstream plans.
//! Actual capacity is derived from member quota observations, not a stored sum
//! of percentages. Quota rows owned by this pool define provisioned budgets.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_pools")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    #[sea_orm(has_many)]
    pub members: HasMany<super::pool_member::Entity>,
    #[sea_orm(has_many)]
    pub plans: HasMany<super::plan::Entity>,
    #[sea_orm(has_many)]
    pub quotas: HasMany<crate::entity::limits::quota::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
