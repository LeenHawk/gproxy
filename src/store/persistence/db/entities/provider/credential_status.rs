//! `credential_statuses` table SeaORM entity (audit snapshot per channel).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_statuses")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub credential_id: i64,
    pub channel: String,
    pub health_kind: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub health_json: Option<String>,
    pub checked_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
