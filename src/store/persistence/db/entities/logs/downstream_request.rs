//! `downstream_requests` table SeaORM entity (raw client → proxy request log).
//! `headers_json` is stored as serialized JSON text.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "downstream_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub status: i64,
    #[sea_orm(column_type = "Text", nullable)]
    pub headers_json: Option<String>,
    pub body: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub response_body: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
