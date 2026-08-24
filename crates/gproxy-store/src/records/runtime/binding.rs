use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingInput {
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub kind: String,
    pub resource_id: String,
    pub credential_id: i64,
    pub summary: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingRecord {
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub kind: String,
    pub resource_id: String,
    pub credential_id: i64,
    pub summary: Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingPage {
    pub items: Vec<BindingRecord>,
    pub next_cursor: Option<String>,
}
