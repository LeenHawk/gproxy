use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "global_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub host: String,
    pub port: i32,
    pub proxy: Option<String>,
    pub spoof_emulation: Option<String>,
    pub dsn: String,
    pub data_dir: String,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub updated_at: OffsetDateTime,
}

impl ActiveModelBehavior for ActiveModel {}
