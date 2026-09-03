use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::{AllowedToolsMode, Rest, ToolChoiceMode};
use super::definitions::NamedTool;

mod allowed;
pub use allowed::ResponseAllowedTool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatToolChoice {
    Mode(ToolChoiceMode),
    Allowed(ChatAllowedToolChoice),
    Named(ChatNamedToolChoice),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseToolChoice {
    Mode(ToolChoiceMode),
    Allowed(ResponseAllowedToolChoice),
    Function(ResponseFunctionToolChoice),
    Mcp(ResponseMcpToolChoice),
    Custom(ResponseCustomToolChoice),
    ApplyPatch(ResponseApplyPatchToolChoice),
    Shell(ResponseShellToolChoice),
    Hosted(ResponseHostedToolChoice),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatAllowedToolChoice {
    pub allowed_tools: ChatAllowedTools,
    #[serde(rename = "type")]
    pub type_: AllowedToolsType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatAllowedTools {
    pub mode: AllowedToolsMode,
    pub tools: Vec<Rest>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAllowedToolChoice {
    pub mode: AllowedToolsMode,
    pub tools: Vec<ResponseAllowedTool>,
    #[serde(rename = "type")]
    pub type_: AllowedToolsType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(AllowedToolsType { AllowedTools => "allowed_tools" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatNamedToolChoice {
    Function(ChatNamedFunctionToolChoice),
    Custom(ChatNamedCustomToolChoice),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatNamedFunctionToolChoice {
    #[serde(rename = "type")]
    pub type_: FunctionToolChoiceType,
    pub function: NamedTool,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatNamedCustomToolChoice {
    #[serde(rename = "type")]
    pub type_: CustomToolChoiceType,
    pub custom: NamedTool,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseHostedToolChoice {
    #[serde(rename = "type")]
    pub type_: ResponseHostedToolChoiceType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(ResponseHostedToolChoiceType {
    FileSearch => "file_search",
    WebSearchPreview => "web_search_preview",
    Computer => "computer",
    ComputerUsePreview => "computer_use_preview",
    ComputerUse => "computer_use",
    WebSearchPreview20250311 => "web_search_preview_2025_03_11",
    ImageGeneration => "image_generation",
    CodeInterpreter => "code_interpreter",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseFunctionToolChoice {
    #[serde(rename = "type")]
    pub type_: FunctionToolChoiceType,
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseMcpToolChoice {
    #[serde(rename = "type")]
    pub type_: McpToolChoiceType,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseCustomToolChoice {
    #[serde(rename = "type")]
    pub type_: CustomToolChoiceType,
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseApplyPatchToolChoice {
    #[serde(rename = "type")]
    pub type_: ApplyPatchToolChoiceType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseShellToolChoice {
    #[serde(rename = "type")]
    pub type_: ShellToolChoiceType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(FunctionToolChoiceType { Function => "function" });
strict_string_enum!(CustomToolChoiceType { Custom => "custom" });
strict_string_enum!(McpToolChoiceType { Mcp => "mcp" });
strict_string_enum!(ApplyPatchToolChoiceType { ApplyPatch => "apply_patch" });
strict_string_enum!(ShellToolChoiceType { Shell => "shell" });
