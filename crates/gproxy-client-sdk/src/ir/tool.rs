use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    #[serde(default)]
    pub provider_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ToolOutput {
    Json(Value),
    Blocks(Vec<ToolResultBlock>),
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub output: ToolOutput,
    pub is_error: bool,
    #[serde(default)]
    pub provider_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Tool {
        name: String,
    },
    #[serde(untagged)]
    Unknown(Value),
}
