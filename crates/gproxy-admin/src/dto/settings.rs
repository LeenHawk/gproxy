use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InstanceSettingsDto {
    pub instance_name: String,
    pub proxy: Option<String>,
    pub spoof_emulation: bool,
    pub enable_usage: bool,
    pub enable_tokenizer_vocabs: bool,
    pub enable_tokenizer_download: bool,
    pub default_tokenizer_vocab: Option<String>,
    pub file_upload_max_in_flight: u64,
    pub inherit_system_proxy: bool,
    pub retention_days: Option<u64>,
    pub max_database_size_mb: Option<u64>,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub disable_log_redaction: bool,
    pub traffic_blacklist: super::TrafficBlacklistDto,
    pub traffic_blacklist_defaults: super::TrafficBlacklistDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerVocabDto {
    pub name: String,
    pub size_bytes: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerFetchRequest {
    pub name: String,
}
