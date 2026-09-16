//! Exact public model name -> route mapping; many names may expose one route.
//! A name such as coding/fast derives namespace coding and local name fast at
//! runtime. Namespace is a name index, not a stored group or ownership scope.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "exposed_models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Globally unique public model name; nonempty, matched exactly.
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(indexed)]
    pub route_id: String,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    #[sea_orm(belongs_to, from = "route_id", to = "id", on_delete = "Cascade")]
    pub route: BelongsTo<super::route::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
