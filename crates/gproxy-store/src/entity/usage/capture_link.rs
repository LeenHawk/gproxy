//! Historical many-to-many associations, independent of log/usage retention.
//! No foreign keys: either endpoint's logging may be disabled or pruned.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_links")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub downstream_id: String,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub upstream_id: String,
}
impl ActiveModelBehavior for ActiveModel {}
