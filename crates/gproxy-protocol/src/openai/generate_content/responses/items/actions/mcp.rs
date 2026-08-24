use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDescription {
    // OpenAI documents an MCP tool input schema as unknown.
    pub input_schema: Value,
    pub name: String,
    // OpenAI documents MCP tool annotations as unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
