//! `instance_settings` table SeaORM entity. Per-instance runtime configuration
//! keyed by a unique `instance_name`.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "instance_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub instance_name: String,
    pub proxy: Option<String>,
    pub spoof_emulation: Option<bool>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub disable_log_redaction: bool,
    pub enable_tokenizer_download: bool,
    pub update_channel: Option<String>,
    pub retention_days: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
