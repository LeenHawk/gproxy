use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: String,
    pub severity: Severity,
    pub published_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affects: Option<String>,
    pub content: HashMap<String, LocalizedContent>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalizedContent {
    pub title: String,
    pub body: String,
}

#[derive(Deserialize)]
pub(super) struct Feed {
    pub(super) version: u32,
    pub(super) notifications: Vec<RawNotification>,
}

#[derive(Clone, Deserialize)]
pub(super) struct RawNotification {
    pub(super) id: String,
    pub(super) severity: String,
    pub(super) published_at: String,
    pub(super) expires_at: Option<String>,
    pub(super) affects: Option<String>,
    pub(super) content: HashMap<String, LocalizedContent>,
}
