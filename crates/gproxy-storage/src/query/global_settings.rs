use gproxy_core::UpdateChannel;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSettingsRow {
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
    pub update_channel: UpdateChannel,
    pub updated_at: OffsetDateTime,
}
