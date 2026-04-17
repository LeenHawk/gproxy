use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

fn default_spoof_emulation() -> String {
    "chrome_136".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettingsWrite {
    pub host: String,
    pub port: u16,
    pub proxy: Option<String>,
    #[serde(default = "default_spoof_emulation")]
    pub spoof_emulation: String,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub dsn: String,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderWrite {
    pub id: i64,
    pub name: String,
    pub channel: String,
    #[serde(default)]
    pub label: Option<String>,
    pub settings_json: String,
    pub routing_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialWrite {
    pub id: i64,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub secret_json: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialStatusKey {
    pub credential_id: i64,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStatusWrite {
    pub id: Option<i64>,
    pub credential_id: i64,
    pub channel: String,
    pub health_kind: String,
    pub health_json: Option<String>,
    pub checked_at_unix_ms: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWrite {
    pub id: i64,
    pub name: String,
    pub password: String,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserKeyWrite {
    pub id: i64,
    pub user_id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownstreamRequestWrite {
    pub trace_id: i64,
    pub at_unix_ms: i64,
    pub internal: bool,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub request_method: String,
    pub request_headers_json: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<i32>,
    pub response_headers_json: String,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamRequestWrite {
    pub downstream_trace_id: Option<i64>,
    pub at_unix_ms: i64,
    pub internal: bool,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub request_method: String,
    pub request_headers_json: String,
    pub request_url: Option<String>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<i32>,
    pub response_headers_json: String,
    pub response_body: Option<Vec<u8>>,
    /// Upstream TTFB in ms for the final attempt. `None` only for rows
    /// written before this feature existed.
    pub initial_latency_ms: Option<i64>,
    /// Upstream total latency in ms for the final attempt. `None` only for
    /// rows written before this feature existed.
    pub total_latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWrite {
    pub downstream_trace_id: Option<i64>,
    pub at_unix_ms: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub protocol: String,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_creation_input_tokens_5min: Option<i64>,
    pub cache_creation_input_tokens_1h: Option<i64>,
    /// Computed cost for quota tracking. When non-zero, the usage sink
    /// will atomically update user_quotas.cost_used in the same DB transaction.
    #[serde(default)]
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelWrite {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub pricing_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelPermissionWrite {
    pub id: i64,
    pub user_id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFilePermissionWrite {
    pub id: i64,
    pub user_id: i64,
    pub provider_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRateLimitWrite {
    pub id: i64,
    pub user_id: i64,
    pub model_pattern: String,
    pub rpm: Option<i32>,
    pub rpd: Option<i32>,
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserQuotaWrite {
    pub user_id: i64,
    pub quota: f64,
    pub cost_used: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserCredentialFileKey {
    pub user_id: i64,
    pub provider_id: i64,
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCredentialFileWrite {
    pub user_id: i64,
    pub user_key_id: i64,
    pub provider_id: i64,
    pub credential_id: i64,
    pub file_id: String,
    pub active: bool,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub deleted_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaudeFileKey {
    pub provider_id: i64,
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeFileWrite {
    pub provider_id: i64,
    pub file_id: String,
    pub file_created_at: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub downloadable: Option<bool>,
    pub raw_json: String,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageWriteEvent {
    UpsertGlobalSettings(GlobalSettingsWrite),
    UpsertProvider(ProviderWrite),
    DeleteProvider { id: i64 },
    UpsertCredential(CredentialWrite),
    DeleteCredential { id: i64 },
    UpsertCredentialStatus(CredentialStatusWrite),
    DeleteCredentialStatus { id: i64 },
    UpsertUser(UserWrite),
    DeleteUser { id: i64 },
    UpsertUserKey(UserKeyWrite),
    DeleteUserKey { id: i64 },
    UpsertModel(ModelWrite),
    DeleteModel { id: i64 },
    UpsertUserModelPermission(UserModelPermissionWrite),
    DeleteUserModelPermission { id: i64 },
    UpsertUserFilePermission(UserFilePermissionWrite),
    DeleteUserFilePermission { id: i64 },
    UpsertUserRateLimit(UserRateLimitWrite),
    DeleteUserRateLimit { id: i64 },
    UpsertUserQuota(UserQuotaWrite),
    UpsertUserCredentialFile(UserCredentialFileWrite),
    UpsertClaudeFile(ClaudeFileWrite),
    UpsertDownstreamRequest(DownstreamRequestWrite),
    UpsertUpstreamRequest(UpstreamRequestWrite),
    UpsertUsage(UsageWrite),
}

#[derive(Debug, Default)]
pub struct StorageWriteBatch {
    pub event_count: usize,
    pub global_settings: Option<GlobalSettingsWrite>,
    pub providers_upsert: HashMap<i64, ProviderWrite>,
    pub providers_delete: HashSet<i64>,
    pub credentials_upsert: HashMap<i64, CredentialWrite>,
    pub credentials_delete: HashSet<i64>,
    pub credential_statuses_upsert: HashMap<CredentialStatusKey, CredentialStatusWrite>,
    pub credential_statuses_delete: HashSet<i64>,
    pub users_upsert: HashMap<i64, UserWrite>,
    pub users_delete: HashSet<i64>,
    pub user_keys_upsert: HashMap<String, UserKeyWrite>,
    pub user_keys_delete: HashSet<i64>,
    pub models_upsert: HashMap<i64, ModelWrite>,
    pub models_delete: HashSet<i64>,
    pub user_model_permissions_upsert: HashMap<i64, UserModelPermissionWrite>,
    pub user_model_permissions_delete: HashSet<i64>,
    pub user_file_permissions_upsert: HashMap<i64, UserFilePermissionWrite>,
    pub user_file_permissions_delete: HashSet<i64>,
    pub user_rate_limits_upsert: HashMap<i64, UserRateLimitWrite>,
    pub user_rate_limits_delete: HashSet<i64>,
    pub user_quotas_upsert: HashMap<i64, UserQuotaWrite>,
    pub user_credential_files_upsert: HashMap<UserCredentialFileKey, UserCredentialFileWrite>,
    pub claude_files_upsert: HashMap<ClaudeFileKey, ClaudeFileWrite>,
    pub downstream_requests_upsert: Vec<DownstreamRequestWrite>,
    pub upstream_requests_upsert: Vec<UpstreamRequestWrite>,
    pub usages_upsert: Vec<UsageWrite>,
}

impl StorageWriteBatch {
    pub fn apply(&mut self, event: StorageWriteEvent) {
        self.event_count += 1;
        match event {
            StorageWriteEvent::UpsertGlobalSettings(value) => {
                self.global_settings = Some(value);
            }
            StorageWriteEvent::UpsertProvider(value) => {
                self.providers_delete.remove(&value.id);
                self.providers_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteProvider { id } => {
                self.providers_upsert.remove(&id);
                self.providers_delete.insert(id);
            }
            StorageWriteEvent::UpsertCredential(value) => {
                self.credentials_delete.remove(&value.id);
                self.credentials_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteCredential { id } => {
                self.credentials_upsert.remove(&id);
                self.credentials_delete.insert(id);
            }
            StorageWriteEvent::UpsertCredentialStatus(value) => {
                let key = CredentialStatusKey {
                    credential_id: value.credential_id,
                    channel: value.channel.clone(),
                };
                if let Some(id) = value.id {
                    self.credential_statuses_delete.remove(&id);
                }
                self.credential_statuses_upsert.insert(key, value);
            }
            StorageWriteEvent::DeleteCredentialStatus { id } => {
                self.credential_statuses_upsert
                    .retain(|_, value| value.id != Some(id));
                self.credential_statuses_delete.insert(id);
            }
            StorageWriteEvent::UpsertUser(value) => {
                self.users_delete.remove(&value.id);
                self.users_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteUser { id } => {
                self.users_upsert.remove(&id);
                self.users_delete.insert(id);
            }
            StorageWriteEvent::UpsertUserKey(value) => {
                self.user_keys_delete.remove(&value.id);
                self.user_keys_upsert
                    .retain(|api_key, row| row.id != value.id || api_key == &value.api_key);
                self.user_keys_upsert.insert(value.api_key.clone(), value);
            }
            StorageWriteEvent::DeleteUserKey { id } => {
                self.user_keys_upsert.retain(|_, row| row.id != id);
                self.user_keys_delete.insert(id);
            }
            StorageWriteEvent::UpsertModel(value) => {
                self.models_delete.remove(&value.id);
                self.models_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteModel { id } => {
                self.models_upsert.remove(&id);
                self.models_delete.insert(id);
            }
            StorageWriteEvent::UpsertUserModelPermission(value) => {
                self.user_model_permissions_delete.remove(&value.id);
                self.user_model_permissions_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteUserModelPermission { id } => {
                self.user_model_permissions_upsert.remove(&id);
                self.user_model_permissions_delete.insert(id);
            }
            StorageWriteEvent::UpsertUserFilePermission(value) => {
                self.user_file_permissions_delete.remove(&value.id);
                self.user_file_permissions_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteUserFilePermission { id } => {
                self.user_file_permissions_upsert.remove(&id);
                self.user_file_permissions_delete.insert(id);
            }
            StorageWriteEvent::UpsertUserRateLimit(value) => {
                self.user_rate_limits_delete.remove(&value.id);
                self.user_rate_limits_upsert.insert(value.id, value);
            }
            StorageWriteEvent::DeleteUserRateLimit { id } => {
                self.user_rate_limits_upsert.remove(&id);
                self.user_rate_limits_delete.insert(id);
            }
            StorageWriteEvent::UpsertUserQuota(value) => {
                self.user_quotas_upsert.insert(value.user_id, value);
            }
            StorageWriteEvent::UpsertUserCredentialFile(value) => {
                let key = UserCredentialFileKey {
                    user_id: value.user_id,
                    provider_id: value.provider_id,
                    file_id: value.file_id.clone(),
                };
                self.user_credential_files_upsert.insert(key, value);
            }
            StorageWriteEvent::UpsertClaudeFile(value) => {
                let key = ClaudeFileKey {
                    provider_id: value.provider_id,
                    file_id: value.file_id.clone(),
                };
                self.claude_files_upsert.insert(key, value);
            }
            StorageWriteEvent::UpsertDownstreamRequest(value) => {
                self.downstream_requests_upsert.push(value);
            }
            StorageWriteEvent::UpsertUpstreamRequest(value) => {
                self.upstream_requests_upsert.push(value);
            }
            StorageWriteEvent::UpsertUsage(value) => {
                self.usages_upsert.push(value);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.event_count == 0
    }
}
