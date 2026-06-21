use serde::{Deserialize, Serialize};

use super::super::{CacheControl, CitationConfig, ClaudeModel, McpToolset, TypedObject};
use super::{CustomToolType, JsonSchema, ToolCommon, ToolCommonWithoutInputExamples, UserLocation};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tool {
    WebFetch(WebFetchTool),
    WebSearch(WebSearchTool),
    Advisor(AdvisorTool),
    Computer(ComputerTool),
    TextEditor(TextEditorTool),
    Command(CommandTool),
    McpToolset(McpToolset),
    Custom(CustomTool),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandTool {
    Bash20241022(BashTool20241022),
    Bash20250124(BashTool20250124),
    CodeExecution20250522(CodeExecutionTool20250522),
    CodeExecution20250825(CodeExecutionTool20250825),
    CodeExecution20260120(CodeExecutionTool20260120),
    Memory20250818(MemoryTool20250818),
    ToolSearchBm25(ToolSearchBm25Tool),
    ToolSearchRegex(ToolSearchRegexTool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BashTool20241022 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub type_: BashTool20241022Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BashTool20250124 {
    pub name: BashToolName,
    #[serde(rename = "type")]
    pub type_: BashTool20250124Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionTool20250522 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250522Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionTool20250825 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250825Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionTool20260120 {
    pub name: CodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20260120Type,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryTool20250818 {
    pub name: MemoryToolName,
    #[serde(rename = "type")]
    pub type_: MemoryTool20250818Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchBm25Tool {
    pub name: ToolSearchBm25ToolName,
    #[serde(rename = "type")]
    pub type_: ToolSearchBm25ToolType,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchRegexTool {
    pub name: ToolSearchRegexToolName,
    #[serde(rename = "type")]
    pub type_: ToolSearchRegexToolType,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextEditorTool {
    TextEditor20241022(TextEditorTool20241022),
    TextEditor20250124(TextEditorTool20250124),
    TextEditor20250429(TextEditorTool20250429),
    TextEditor20250728(TextEditorTool20250728),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorTool20241022 {
    pub name: StrReplaceEditorToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20241022Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorTool20250124 {
    pub name: StrReplaceEditorToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250124Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorTool20250429 {
    pub name: StrReplaceBasedEditToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250429Type,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditorTool20250728 {
    pub name: StrReplaceBasedEditToolName,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250728Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u64>,
    #[serde(flatten)]
    pub common: ToolCommon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComputerTool {
    Computer20241022(ComputerTool20241022),
    Computer20250124(ComputerTool20250124),
    Computer20251124(ComputerTool20251124),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSearchTool {
    WebSearch20250305(WebSearchTool20250305),
    WebSearch20260209(WebSearchTool20260209),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchTool20250305 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20250305Type,
    #[serde(flatten)]
    pub params: WebSearchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchTool20260209 {
    pub name: WebSearchToolName,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20260209Type,
    #[serde(flatten)]
    pub params: WebSearchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchToolParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebFetchTool {
    WebFetch20250910(WebFetchTool20250910),
    WebFetch20260209(WebFetchTool20260209),
    WebFetch20260309(WebFetchTool20260309),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebFetchTool20250910 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20250910Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebFetchTool20260209 {
    pub name: WebFetchToolName,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260209Type,
    #[serde(flatten)]
    pub params: WebFetchToolParams,
    #[serde(flatten)]
    pub common: ToolCommonWithoutInputExamples,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

macro_rules! single_wire_enum {
    ($name:ident { $variant:ident => $wire:literal }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

single_wire_enum!(BashToolName { Bash => "bash" });
single_wire_enum!(BashTool20241022Type { Bash20241022 => "bash_20241022" });
single_wire_enum!(BashTool20250124Type { Bash20250124 => "bash_20250124" });
single_wire_enum!(CodeExecutionToolName { CodeExecution => "code_execution" });
single_wire_enum!(CodeExecutionTool20250522Type { CodeExecution20250522 => "code_execution_20250522" });
single_wire_enum!(CodeExecutionTool20250825Type { CodeExecution20250825 => "code_execution_20250825" });
single_wire_enum!(CodeExecutionTool20260120Type { CodeExecution20260120 => "code_execution_20260120" });
single_wire_enum!(MemoryToolName { Memory => "memory" });
single_wire_enum!(MemoryTool20250818Type { Memory20250818 => "memory_20250818" });
single_wire_enum!(ToolSearchBm25ToolName { ToolSearchBm25 => "tool_search_tool_bm25" });
single_wire_enum!(ToolSearchRegexToolName { ToolSearchRegex => "tool_search_tool_regex" });
single_wire_enum!(StrReplaceEditorToolName { StrReplaceEditor => "str_replace_editor" });
single_wire_enum!(StrReplaceBasedEditToolName { StrReplaceBasedEditTool => "str_replace_based_edit_tool" });
single_wire_enum!(TextEditorTool20241022Type { TextEditor20241022 => "text_editor_20241022" });
single_wire_enum!(TextEditorTool20250124Type { TextEditor20250124 => "text_editor_20250124" });
single_wire_enum!(TextEditorTool20250429Type { TextEditor20250429 => "text_editor_20250429" });
single_wire_enum!(TextEditorTool20250728Type { TextEditor20250728 => "text_editor_20250728" });
single_wire_enum!(ComputerToolName { Computer => "computer" });
single_wire_enum!(ComputerTool20241022Type { Computer20241022 => "computer_20241022" });
single_wire_enum!(ComputerTool20250124Type { Computer20250124 => "computer_20250124" });
single_wire_enum!(ComputerTool20251124Type { Computer20251124 => "computer_20251124" });
single_wire_enum!(WebSearchToolName { WebSearch => "web_search" });
single_wire_enum!(WebSearchTool20250305Type { WebSearch20250305 => "web_search_20250305" });
single_wire_enum!(WebSearchTool20260209Type { WebSearch20260209 => "web_search_20260209" });
single_wire_enum!(WebFetchToolName { WebFetch => "web_fetch" });
single_wire_enum!(WebFetchTool20250910Type { WebFetch20250910 => "web_fetch_20250910" });
single_wire_enum!(WebFetchTool20260209Type { WebFetch20260209 => "web_fetch_20260209" });
single_wire_enum!(WebFetchTool20260309Type { WebFetch20260309 => "web_fetch_20260309" });
single_wire_enum!(AdvisorToolName { Advisor => "advisor" });
single_wire_enum!(AdvisorTool20260301Type { Advisor20260301 => "advisor_20260301" });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSearchBm25ToolType {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchBm2520251119,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchBm25,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSearchRegexToolType {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchRegex20251119,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchRegex,
}
