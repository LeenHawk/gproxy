use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "codex_task_bindings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub provider_id: i64,
    #[sea_orm(column_type = "Text")]
    pub task_id: String,
    pub credential_id: i64,
    pub owner_user_id: i64,
    #[sea_orm(column_type = "Text", nullable)]
    pub environment_id: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub summary_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
