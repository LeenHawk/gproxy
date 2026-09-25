//! A public model name with its own provider/model members, strategy and attempt budget.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "routes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(default_value = "round_robin")]
    pub strategy: RouteStrategy,
    /// Reuse a successful provider/model target for a stable caller session.
    #[sea_orm(default_value = false)]
    pub session_affinity: bool,
    /// Positive total attempt budget, including the initial call; the global
    /// Setting.max_attempts remains an upper bound during execution.
    #[sea_orm(default_value = 6)]
    pub max_attempts: u32,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(has_many)]
    pub members: HasMany<super::route_member::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// Strategies apply to members in the preferred available tier/health group;
/// credential selection within the chosen provider is a separate decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum RouteStrategy {
    #[sea_orm(string_value = "round_robin")]
    RoundRobin,
    #[sea_orm(string_value = "weighted")]
    Weighted,
    /// Keeps candidate order: tier, health, descending weight, stable ID.
    #[sea_orm(string_value = "failover")]
    Failover,
}
