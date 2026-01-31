use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_disallow")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "credential_scope")]
    pub credential_id: i64,
    #[sea_orm(unique_key = "credential_scope")]
    pub scope_kind: String,
    #[sea_orm(unique_key = "credential_scope")]
    pub scope_value: Option<String>,
    pub level: String,
    pub until_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
    pub updated_at: OffsetDateTime,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "Cascade")]
    pub credential: HasOne<super::credentials::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
