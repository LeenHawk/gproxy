use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialHealthState {
    Healthy,
    Degraded,
    Dead,
}

impl CredentialHealthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Dead => "dead",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "healthy" => Some(Self::Healthy),
            "degraded" => Some(Self::Degraded),
            "dead" => Some(Self::Dead),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialHealthInput {
    pub credential_id: i64,
    pub credential_version: u64,
    pub version: i64,
    pub state: CredentialHealthState,
    pub observed_at: i64,
    pub response_status: Option<u16>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialHealthRecord {
    pub credential_id: i64,
    pub credential_version: u64,
    pub version: i64,
    pub state: CredentialHealthState,
    pub observed_at: i64,
    pub response_status: Option<u16>,
    pub detail: Option<String>,
}
