use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum UpdateChannelDto {
    Releases,
    Staging,
    Dev,
}

impl UpdateChannelDto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Releases => "releases",
            Self::Staging => "staging",
            Self::Dev => "dev",
        }
    }

    pub fn from_stored(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "releases" | "release" | "stable" => Some(Self::Releases),
            "staging" => Some(Self::Staging),
            "dev" | "development" => Some(Self::Dev),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InstanceSettingsDto {
    pub instance_name: String,
    pub proxy: Option<String>,
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
    pub update_channel: Option<UpdateChannelDto>,
    pub enable_auto_update_check: bool,
    pub traffic_blacklist: super::TrafficBlacklistDto,
    pub traffic_blacklist_defaults: super::TrafficBlacklistDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerVocabDto {
    pub name: String,
    pub repository: String,
    pub size_bytes: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerFetchRequest {
    pub name: String,
    pub repository: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerDeleteRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerDownloadProgressDto {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerAuthDto {
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerAuthUpdate {
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TokenizerAuthRevealResponse {
    pub token: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn update_channel_wire_values_are_closed() {
        assert_eq!(
            serde_json::from_str::<super::UpdateChannelDto>("\"dev\"").unwrap(),
            super::UpdateChannelDto::Dev
        );
        assert!(serde_json::from_str::<super::UpdateChannelDto>("\"nightly\"").is_err());
    }
}
