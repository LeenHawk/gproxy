use serde::{Deserialize, Serialize};

use super::super::{CacheControl, CitationConfig, ClaudeModel, McpToolset, ResponseInclusion};
use super::wire::*;
use super::{CustomToolType, JsonSchema, ToolCommon, ToolCommonWithoutInputExamples, UserLocation};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Tool {
    WebFetch(WebFetchTool),
    WebSearch(WebSearchTool),
    Advisor(AdvisorTool),
    Computer(ComputerTool),
    TextEditor(TextEditorTool),
    Command(CommandTool),
    McpToolset(McpToolset),
    Custom(CustomTool),
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomTool {
    pub input_schema: JsonSchema,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<CustomToolType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CommandTool {
    Bash20241022(BashTool20241022),
    Bash20250124(BashTool20250124),
    CodeExecution20250522(CodeExecutionTool20250522),
    CodeExecution20250825(CodeExecutionTool20250825),
    CodeExecution20260120(CodeExecutionTool20260120),
    CodeExecution20260521(CodeExecutionTool20260521),
    Memory20250818(MemoryTool20250818),
    ToolSearchBm25(ToolSearchBm25Tool),
    ToolSearchRegex(ToolSearchRegexTool),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BashTool20241022 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub type_: BashTool20241022Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BashTool20250124 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub type_: BashTool20250124Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecutionTool20250522 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250522Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecutionTool20250825 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250825Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecutionTool20260120 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20260120Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecutionTool20260521 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20260521Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MemoryTool20250818 {
    pub name: MemoryToolName,
    #[serde(rename = "type")]
    pub type_: MemoryTool20250818Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolSearchBm25Tool {
    pub name: ToolSearchBm25ToolName,
    #[serde(rename = "type")]
    pub type_: ToolSearchBm25ToolType,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolSearchRegexTool {
    pub name: ToolSearchRegexToolName,
    #[serde(rename = "type")]
    pub type_: ToolSearchRegexToolType,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum TextEditorTool {
    TextEditor20241022(TextEditorTool20241022),
    TextEditor20250124(TextEditorTool20250124),
    TextEditor20250429(TextEditorTool20250429),
    TextEditor20250728(TextEditorTool20250728),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextEditorTool20241022 {
    pub name: StrReplaceEditorToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20241022Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextEditorTool20250124 {
    pub name: StrReplaceEditorToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250124Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextEditorTool20250429 {
    pub name: StrReplaceBasedEditToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250429Type,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextEditorTool20250728 {
    pub name: StrReplaceBasedEditToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250728Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u64>,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ComputerTool {
    Computer20241022(ComputerTool20241022),
    Computer20250124(ComputerTool20250124),
    Computer20251124(ComputerTool20251124),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerTool20241022 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub type_: ComputerTool20241022Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerTool20250124 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub type_: ComputerTool20250124Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerTool20251124 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: ComputerToolName,
    #[serde(rename = "type")]
    pub type_: ComputerTool20251124Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
    #[serde(flatten)]
    pub common: ToolCommon,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum WebSearchTool {
    WebSearch20250305(WebSearchTool20250305),
    WebSearch20260209(WebSearchTool20260209),
    WebSearch20260318(WebSearchTool20260318),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchTool20250305 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20250305Type,
    #[serde(flatten)]
    pub params: WebSearchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchTool20260209 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20260209Type,
    #[serde(flatten)]
    pub params: WebSearchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchTool20260318 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20260318Type,
    #[serde(flatten)]
    pub params: WebSearchToolParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_inclusion: Option<ResponseInclusion>,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchToolParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum WebFetchTool {
    WebFetch20250910(WebFetchTool20250910),
    WebFetch20260209(WebFetchTool20260209),
    WebFetch20260309(WebFetchTool20260309),
    WebFetch20260318(WebFetchTool20260318),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchTool20250910 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20250910Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchTool20260209 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260209Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchTool20260309 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260309Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_cache: Option<bool>,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchTool20260318 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260318Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_inclusion: Option<ResponseInclusion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_cache: Option<bool>,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchToolParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct AdvisorTool {
    pub model: ClaudeModel,
    pub name: AdvisorToolName,
    #[serde(rename = "type")]
    pub type_: AdvisorTool20260301Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caching: Option<CacheControl>,
    /// Bounds the advisor's total output (thinking + text) per call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
