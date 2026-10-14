//! Capture payload storage; resolved only through an authorized capture read.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "header_sets")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub hash: String,
    pub headers: Json,
}

impl ActiveModelBehavior for ActiveModel {}
