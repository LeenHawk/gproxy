use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::{JsonSchema, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LegacyFunctionCallChoice {
    Mode(LegacyFunctionCallMode),
    Named(LegacyFunctionCallOption),
    Unknown(Value),
}

strict_string_enum!(LegacyFunctionCallMode { None => "none", Auto => "auto" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyFunctionCallOption {
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyFunctionDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
