use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct AutostartStatusDto {
    pub supported: bool,
    pub enabled: bool,
    pub platform: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct AutostartUpdateRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct UpdateStatusDto {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub channel: String,
    pub target: String,
    pub notes: Option<String>,
    pub rollback_available: bool,
    pub restart: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct UpdateAppliedDto {
    pub version: String,
    pub restart: String,
}
