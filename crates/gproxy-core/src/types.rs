//! Shared domain types mirrored from database records into memory.

use serde::{Deserialize, Serialize};

/// Self-update release channel.
///
/// `release` tracks the latest stable GitHub release; `staging` tracks the
/// long-lived `staging` tag, refreshed continuously for preview builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum UpdateChannel {
    #[default]
    Release,
    Staging,
}

impl UpdateChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateChannel::Release => "release",
            UpdateChannel::Staging => "staging",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "staging" => UpdateChannel::Staging,
            _ => UpdateChannel::Release,
        }
    }
}

/// In-memory user record for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUser {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub is_admin: bool,
    #[serde(skip)]
    pub password_hash: String,
}

/// In-memory API key record for fast authentication lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUserKey {
    pub id: i64,
    pub user_id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

/// In-memory model record (from models table).
#[derive(Debug, Clone)]
pub struct MemoryModel {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub pricing: Option<gproxy_sdk::channel::billing::ModelPrice>,
}

/// In-memory user credential file record.
#[derive(Debug, Clone)]
pub struct MemoryUserCredentialFile {
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_id: i64,
    pub credential_id: i64,
    pub file_id: String,
    pub active: bool,
    pub created_at_unix_ms: i64,
}

/// In-memory Claude file metadata record.
#[derive(Debug, Clone)]
pub struct MemoryClaudeFile {
    pub provider_id: i64,
    pub file_id: String,
    pub file_created_at_unix_ms: i64,
    pub metadata: gproxy_sdk::protocol::claude::types::FileMetadata,
}

/// Model alias resolution target.
#[derive(Debug, Clone)]
pub struct ModelAliasTarget {
    pub provider_name: String,
    pub model_id: String,
}

/// Permission entry for model access control.
#[derive(Debug, Clone)]
pub struct PermissionEntry {
    pub id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
}

/// File permission entry for upload capability.
#[derive(Debug, Clone)]
pub struct FilePermissionEntry {
    pub id: i64,
    pub provider_id: i64,
}

/// Rate limit rule per user per model pattern.
#[derive(Debug, Clone)]
pub struct RateLimitRule {
    pub id: i64,
    pub model_pattern: String,
    pub rpm: Option<i32>,
    pub rpd: Option<i32>,
    pub total_tokens: Option<i64>,
}

/// Server-wide configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default = "default_spoof_emulation")]
    pub spoof_emulation: String,
    #[serde(default = "default_true")]
    pub enable_usage: bool,
    #[serde(default = "default_false")]
    pub enable_upstream_log: bool,
    #[serde(default = "default_false")]
    pub enable_upstream_log_body: bool,
    #[serde(default = "default_false")]
    pub enable_downstream_log: bool,
    #[serde(default = "default_false")]
    pub enable_downstream_log_body: bool,
    pub dsn: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub update_channel: UpdateChannel,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            proxy: None,
            spoof_emulation: default_spoof_emulation(),
            enable_usage: true,
            enable_upstream_log: false,
            enable_upstream_log_body: false,
            enable_downstream_log: false,
            enable_downstream_log_body: false,
            dsn: String::new(),
            data_dir: default_data_dir(),
            update_channel: UpdateChannel::default(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8787
}
fn default_spoof_emulation() -> String {
    "chrome_136".to_string()
}
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_data_dir() -> String {
    "./data".to_string()
}
