//! Observed upstream quota cycles. credential_id remains a historical reference after credential deletion.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_quota_cycles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub credential_id: String,
    pub scope: Json,
    pub snapshot: Json,
    pub observed_at_ms: i64,
    pub starts_at_ms: Option<i64>,
    pub resets_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
