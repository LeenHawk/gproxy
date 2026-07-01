use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{BoolOrStringArray, JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextManagementConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<ContextEdit>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextEdit {
    Known(KnownContextEdit),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KnownContextEdit {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses {
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_at_least: Option<InputTokensValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_tool_inputs: Option<BoolOrStringArray>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exclude_tools: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<ToolUsesValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger: Option<ContextTrigger>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking {
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<ThinkingKeep>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "compact_20260112")]
    Compact {
        #[serde(skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pause_after_compaction: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger: Option<InputTokensValue>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTokensValue {
    #[serde(rename = "type")]
    pub type_: InputTokensValueType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputTokensValueType {
    #[serde(rename = "input_tokens")]
    InputTokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUsesValue {
    #[serde(rename = "type")]
    pub type_: ToolUsesValueType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolUsesValueType {
    #[serde(rename = "tool_uses")]
    ToolUses,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextTrigger {
    InputTokens(InputTokensValue),
    ToolUses(ToolUsesValue),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThinkingKeep {
    Object(ThinkingKeepObject),
    All(ThinkingAllValue),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ThinkingKeepObject {
    #[serde(rename = "thinking_turns")]
    ThinkingTurns { value: u64 },
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinkingAllValue {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextManagementResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_edits: Option<Vec<AppliedContextEdit>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AppliedContextEdit {
    Known(KnownAppliedContextEdit),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KnownAppliedContextEdit {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses {
        cleared_input_tokens: u64,
        cleared_tool_uses: u64,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking {
        cleared_input_tokens: u64,
        cleared_thinking_turns: u64,
    },
}
