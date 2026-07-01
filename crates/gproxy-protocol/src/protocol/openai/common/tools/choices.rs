use serde::{Deserialize, Serialize};

use super::super::{AllowedToolsMode, Extra, ToolChoiceMode};
use super::definitions::NamedTool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatToolChoice {
    Mode(ToolChoiceMode),
    Allowed(ChatAllowedToolChoice),
    Named(ChatNamedToolChoice),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseToolChoice {
    Mode(ToolChoiceMode),
    Allowed(ResponseAllowedToolChoice),
    Hosted(ResponseHostedToolChoice),
    Function(ResponseFunctionToolChoice),
    Mcp(ResponseMcpToolChoice),
    Custom(ResponseCustomToolChoice),
    ApplyPatch(ResponseApplyPatchToolChoice),
    Shell(ResponseShellToolChoice),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatAllowedToolChoice {
    pub allowed_tools: ChatAllowedTools,
    #[serde(rename = "type")]
    pub type_: AllowedToolsType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatAllowedTools {
    pub mode: AllowedToolsMode,
    pub tools: Vec<Extra>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAllowedToolChoice {
    pub mode: AllowedToolsMode,
    pub tools: Vec<ResponseAllowedTool>,
    #[serde(rename = "type")]
    pub type_: AllowedToolsType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseAllowedTool {
    #[serde(rename = "function")]
    Function {
        name: String,
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "custom")]
    Custom {
        name: String,
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "mcp")]
    Mcp {
        server_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "file_search")]
    FileSearch {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "web_search_preview")]
    WebSearchPreview {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "computer")]
    Computer {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "computer_use")]
    ComputerUse {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311 {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "image_generation")]
    ImageGeneration {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "code_interpreter")]
    CodeInterpreter {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "local_shell")]
    LocalShell {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "shell")]
    Shell {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    #[serde(rename = "apply_patch")]
    ApplyPatch {
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllowedToolsType {
    #[serde(rename = "allowed_tools")]
    AllowedTools,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatNamedToolChoice {
    Function {
        #[serde(rename = "type")]
        type_: FunctionToolChoiceType,
        function: NamedTool,
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
    Custom {
        #[serde(rename = "type")]
        type_: CustomToolChoiceType,
        custom: NamedTool,
        #[serde(
            default,
            flatten,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseHostedToolChoice {
    #[serde(rename = "type")]
    pub type_: ResponseHostedToolChoiceType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseHostedToolChoiceType {
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "computer")]
    Computer,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
    #[serde(rename = "computer_use")]
    ComputerUse,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    #[serde(rename = "image_generation")]
    ImageGeneration,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionToolChoice {
    #[serde(rename = "type")]
    pub type_: FunctionToolChoiceType,
    pub name: String,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpToolChoice {
    #[serde(rename = "type")]
    pub type_: McpToolChoiceType,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCustomToolChoice {
    #[serde(rename = "type")]
    pub type_: CustomToolChoiceType,
    pub name: String,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseApplyPatchToolChoice {
    #[serde(rename = "type")]
    pub type_: ApplyPatchToolChoiceType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellToolChoice {
    #[serde(rename = "type")]
    pub type_: ShellToolChoiceType,
    #[serde(
        default,
        flatten,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionToolChoiceType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CustomToolChoiceType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpToolChoiceType {
    #[serde(rename = "mcp")]
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyPatchToolChoiceType {
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellToolChoiceType {
    #[serde(rename = "shell")]
    Shell,
}
