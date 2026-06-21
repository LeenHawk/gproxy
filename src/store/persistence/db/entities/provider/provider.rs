//! `providers` table SeaORM entity. `settings_json` is stored as serialized
//! JSON text for dialect portability (sqlite/pg/mysql).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "providers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub name: String,
    pub channel: String,
    pub label: Option<String>,
    #[sea_orm(column_type = "Text")]
    pub settings_json: String,
    pub credential_strategy: String,
    pub proxy_url: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub tls_fingerprint: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
