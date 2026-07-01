use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::{JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    Auto(ToolChoiceAuto),
    Any(ToolChoiceAny),
    Tool(ToolChoiceTool),
    None(ToolChoiceNone),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceAuto {
    #[serde(rename = "type")]
    pub type_: ToolChoiceAutoType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChoiceAutoType {
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceAny {
    #[serde(rename = "type")]
    pub type_: ToolChoiceAnyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChoiceAnyType {
    #[serde(rename = "any")]
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceTool {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ToolChoiceToolType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChoiceToolType {
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceNone {
    #[serde(rename = "type")]
    pub type_: ToolChoiceNoneType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChoiceNoneType {
    #[serde(rename = "none")]
    None,
}
