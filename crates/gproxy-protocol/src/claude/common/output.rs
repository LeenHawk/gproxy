use serde::{Deserialize, Serialize};

use super::{JsonSchemaFormat, OutputEffort, TaskBudgetType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OutputEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsonSchemaFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_budget: Option<TokenTaskBudget>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemMessageOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OutputEffort>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenTaskBudget {
    pub total: u64,
    #[serde(rename = "type")]
    pub type_: TaskBudgetType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
