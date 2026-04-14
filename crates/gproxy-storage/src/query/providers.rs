use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use super::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderQuery {
    pub channel: Scope<String>,
    pub name: Scope<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderQueryRow {
    pub id: i64,
    pub name: String,
    pub channel: String,
    #[serde(default)]
    pub label: Option<String>,
    pub settings_json: Value,
    pub dispatch_json: Value,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
