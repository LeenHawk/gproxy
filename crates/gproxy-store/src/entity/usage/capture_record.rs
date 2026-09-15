//! A captured body reference; request and call IDs are logical correlations, not retention-coupling FKs.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "capture_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub request_id: String,
    #[sea_orm(indexed)]
    pub upstream_call_id: Option<String>,
    pub kind: String,
    #[sea_orm(indexed)]
    pub file_id: String,
    pub created_at_ms: i64,
    #[sea_orm(belongs_to, from = "file_id", to = "id", on_delete = "Cascade")]
    pub file: BelongsTo<crate::entity::resource::file_object::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
