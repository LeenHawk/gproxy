use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
pub(super) struct Notification {
    pub id: String,
    pub severity: Severity,
    pub published_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affects: Option<String>,
    pub content: HashMap<String, LocalizedContent>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct LocalizedContent {
    pub title: String,
    pub body: String,
}

#[derive(Deserialize)]
pub(super) struct Feed {
    pub version: u32,
    pub notifications: Vec<RawNotification>,
}

#[derive(Clone, Deserialize)]
pub(super) struct RawNotification {
    pub id: String,
    pub severity: String,
    pub published_at: String,
    pub expires_at: Option<String>,
    pub affects: Option<String>,
    #[serde(default)]
    pub channels: Vec<String>,
    pub content: HashMap<String, LocalizedContent>,
}
