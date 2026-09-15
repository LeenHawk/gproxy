//! Client/upstream resource mapping. Scope is supplied by the host resource API.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "resource_bindings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "public_resource")]
    pub scope: String,
    #[sea_orm(unique_key = "public_resource")]
    pub kind: String,
    #[sea_orm(unique_key = "public_resource")]
    pub public_id: String,
    #[sea_orm(column_type = "Text")]
    pub upstream_id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub credential_id: String,
    pub file_id: Option<String>,
    #[sea_orm(indexed)]
    pub expires_at_ms: Option<i64>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<Option<crate::entity::identity::user::Entity>>,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "Cascade")]
    pub credential: BelongsTo<crate::entity::upstream::credential::Entity>,
    #[sea_orm(belongs_to, from = "file_id", to = "id", on_delete = "SetNull")]
    pub file: BelongsTo<Option<super::file_object::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
