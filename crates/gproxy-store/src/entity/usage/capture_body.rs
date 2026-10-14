//! Capture payload storage; resolved only through an authorized capture read.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_bodies")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub upstream_id: Option<String>,
    #[sea_orm(indexed)]
    pub downstream_id: Option<String>,
    pub raw_size: i64,
    #[sea_orm(belongs_to, from = "upstream_id", to = "id", on_delete = "Cascade")]
    pub upstream: BelongsTo<Option<super::upstream_record::Entity>>,
    #[sea_orm(belongs_to, from = "downstream_id", to = "id", on_delete = "Cascade")]
    pub downstream: BelongsTo<Option<super::downstream_record::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
