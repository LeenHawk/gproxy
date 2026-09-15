//! One physical upstream call, including retries and calls inside composed protocol adaptations.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_calls")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Logical correlation; usage recording and call retention can be configured independently.
    #[sea_orm(indexed)]
    pub request_id: String,
    pub sequence: i32,
    #[sea_orm(indexed)]
    pub provider_id: String,
    #[sea_orm(indexed)]
    pub credential_id: String,
    pub model: String,
    pub operation: String,
    pub dialect: String,
    pub http_status: Option<i32>,
    #[sea_orm(column_type = "Text")]
    pub error: Option<String>,
    pub metrics: Option<Json>,
    #[sea_orm(indexed)]
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
