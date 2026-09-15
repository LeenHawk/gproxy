//! File content lives in gproxy-file (filesystem/S3). This table stores only metadata and a locator.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "file_objects")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub storage_name: String,
    #[sea_orm(column_type = "Text")]
    pub object_key: String,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub size_bytes: i64,
    pub created_at_ms: i64,
    #[sea_orm(indexed)]
    pub expires_at_ms: Option<i64>,
    #[sea_orm(has_many)]
    pub models: HasMany<crate::entity::upstream::model::Entity>,
    #[sea_orm(has_many)]
    pub captures: HasMany<crate::entity::usage::capture_record::Entity>,
    #[sea_orm(has_many)]
    pub resource_bindings: HasMany<super::resource_binding::Entity>,
    #[sea_orm(has_many)]
    pub global_settings: HasMany<crate::entity::config::setting::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
