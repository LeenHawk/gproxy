//! Capture payload storage; resolved only through an authorized capture read.
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_body_blobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub body_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub ordinal: i32,
    #[sea_orm(indexed)]
    pub blob_id: String,
    #[sea_orm(belongs_to, from = "body_id", to = "id", on_delete = "Cascade")]
    pub body: BelongsTo<super::capture_body::Entity>,
    #[sea_orm(belongs_to, from = "blob_id", to = "id", on_delete = "Restrict")]
    pub blob: BelongsTo<super::capture_blob::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
