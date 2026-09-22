use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::content::{CacheControl, CitationsConfig};
use crate::Rest;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Tool {
    pub input_schema: JsonSchema,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub type_: Option<ToolType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct JsonSchema {
    #[serde(rename = "type")]
    pub type_: JsonSchemaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<Value>,
    #[serde(rename = "$defs", skip_serializing_if = "Option::is_none")]
    pub defs: Option<Value>,
    #[serde(rename = "definitions", skip_serializing_if = "Option::is_none")]
    pub definitions: Option<Value>,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema_uri: Option<String>,
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(rename = "$anchor", skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(rename = "$dynamicRef", skip_serializing_if = "Option::is_none")]
    pub dynamic_reference: Option<String>,
    #[serde(rename = "$dynamicAnchor", skip_serializing_if = "Option::is_none")]
    pub dynamic_anchor: Option<String>,
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(rename = "title", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "description", skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,
    #[serde(rename = "examples", skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<Value>>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<Value>,
    #[serde(rename = "allOf", skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<Value>>,
    #[serde(rename = "anyOf", skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Value>>,
    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Value>>,
    #[serde(rename = "not", skip_serializing_if = "Option::is_none")]
    pub not: Option<Value>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_schema: Option<Value>,
    #[serde(rename = "then", skip_serializing_if = "Option::is_none")]
    pub then_schema: Option<Value>,
    #[serde(rename = "else", skip_serializing_if = "Option::is_none")]
    pub else_schema: Option<Value>,
    #[serde(rename = "patternProperties", skip_serializing_if = "Option::is_none")]
    pub pattern_properties: Option<Value>,
    #[serde(rename = "propertyNames", skip_serializing_if = "Option::is_none")]
    pub property_names: Option<Value>,
    #[serde(rename = "dependentRequired", skip_serializing_if = "Option::is_none")]
    pub dependent_required: Option<Value>,
    #[serde(rename = "dependentSchemas", skip_serializing_if = "Option::is_none")]
    pub dependent_schemas: Option<Value>,
    #[serde(rename = "dependencies", skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Value>,
    #[serde(
        rename = "unevaluatedProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub unevaluated_properties: Option<Value>,
    #[serde(rename = "minProperties", skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<u64>,
    #[serde(rename = "maxProperties", skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<u64>,
    #[serde(rename = "readOnly", skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(rename = "writeOnly", skip_serializing_if = "Option::is_none")]
    pub write_only: Option<bool>,
    #[serde(rename = "deprecated", skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum JsonSchemaType {
    Object,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolType {
    Custom,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AllowedCaller {
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
    #[serde(rename = "code_execution_20260120")]
    CodeExecution20260120,
    #[serde(rename = "code_execution_20260521")]
    CodeExecution20260521,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[allow(clippy::large_enum_variant)]
pub enum ToolUnion {
    BrowserToolset20260801(BrowserToolset20260801),
    ComputerToolset20260801(ComputerToolset20260801),
    Custom(Tool),
    Mcp(McpToolset),
    Bash20241022(BashTool20241022),
    Bash20250124(BashTool20250124),
    CodeExecution20250522(CodeExecutionTool20250522),
    CodeExecution20250825(CodeExecutionTool20250825),
    CodeExecution20260120(CodeExecutionTool20260120),
    CodeExecution20260521(CodeExecutionTool20260521),
    Computer20241022(ComputerTool20241022),
    Memory20250818(MemoryTool20250818),
    Computer20250124(ComputerTool20250124),
    TextEditor20241022(TextEditorTool20241022),
    Computer20251124(ComputerTool20251124),
    TextEditor20250124(TextEditorTool20250124),
    TextEditor20250429(TextEditorTool20250429),
    TextEditor20250728(TextEditorTool20250728),
    WebSearch20250305(WebSearchTool20250305),
    WebFetch20250910(WebFetchTool20250910),
    WebSearch20260209(WebSearchTool20260209),
    WebFetch20260209(WebFetchTool20260209),
    WebFetch20260309(WebFetchTool20260309),
    WebSearch20260318(WebSearchTool20260318),
    WebFetch20260318(WebFetchTool20260318),
    Advisor20260301(AdvisorTool20260301),
    ToolSearchBm2520251119(ToolSearchBm25Tool20251119),
    ToolSearchRegex20251119(ToolSearchRegexTool20251119),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<McpListedTool>>,

    pub mcp_server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolsetType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<std::collections::BTreeMap<String, McpToolConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<McpToolDefaultConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolDefaultConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpToolsetType {
    McpToolset,
}

type Examples = Vec<Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashTool20241022Name {
    #[serde(rename = "bash")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashTool20241022Type {
    #[serde(rename = "bash_20241022")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashTool20241022 {
    pub name: BashTool20241022Name,
    #[serde(rename = "type")]
    pub type_: BashTool20241022Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashTool20250124Name {
    #[serde(rename = "bash")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashTool20250124Type {
    #[serde(rename = "bash_20250124")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashTool20250124 {
    pub name: BashTool20250124Name,
    #[serde(rename = "type")]
    pub type_: BashTool20250124Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20250522Name {
    #[serde(rename = "code_execution")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20250522Type {
    #[serde(rename = "code_execution_20250522")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionTool20250522 {
    pub name: CodeExecutionTool20250522Name,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250522Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20250825Name {
    #[serde(rename = "code_execution")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20250825Type {
    #[serde(rename = "code_execution_20250825")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionTool20250825 {
    pub name: CodeExecutionTool20250825Name,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20250825Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20260120Name {
    #[serde(rename = "code_execution")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20260120Type {
    #[serde(rename = "code_execution_20260120")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionTool20260120 {
    pub name: CodeExecutionTool20260120Name,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20260120Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20260521Name {
    #[serde(rename = "code_execution")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionTool20260521Type {
    #[serde(rename = "code_execution_20260521")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionTool20260521 {
    pub name: CodeExecutionTool20260521Name,
    #[serde(rename = "type")]
    pub type_: CodeExecutionTool20260521Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MemoryTool20250818Name {
    #[serde(rename = "memory")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MemoryTool20250818Type {
    #[serde(rename = "memory_20250818")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MemoryTool20250818 {
    pub name: MemoryTool20250818Name,
    #[serde(rename = "type")]
    pub type_: MemoryTool20250818Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20241022Name {
    #[serde(rename = "computer")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20241022Type {
    #[serde(rename = "computer_20241022")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerTool20241022 {
    pub name: ComputerTool20241022Name,
    #[serde(rename = "type")]
    pub type_: ComputerTool20241022Type,
    pub display_height_px: i64,
    pub display_width_px: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20250124Name {
    #[serde(rename = "computer")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20250124Type {
    #[serde(rename = "computer_20250124")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerTool20250124 {
    pub name: ComputerTool20250124Name,
    #[serde(rename = "type")]
    pub type_: ComputerTool20250124Type,
    pub display_height_px: i64,
    pub display_width_px: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20251124Name {
    #[serde(rename = "computer")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerTool20251124Type {
    #[serde(rename = "computer_20251124")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerTool20251124 {
    pub name: ComputerTool20251124Name,
    #[serde(rename = "type")]
    pub type_: ComputerTool20251124Type,
    pub display_height_px: i64,
    pub display_width_px: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20241022Name {
    #[serde(rename = "str_replace_editor")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20241022Type {
    #[serde(rename = "text_editor_20241022")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorTool20241022 {
    pub name: TextEditorTool20241022Name,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20241022Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250124Name {
    #[serde(rename = "str_replace_editor")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250124Type {
    #[serde(rename = "text_editor_20250124")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorTool20250124 {
    pub name: TextEditorTool20250124Name,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250124Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250429Name {
    #[serde(rename = "str_replace_based_edit_tool")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250429Type {
    #[serde(rename = "text_editor_20250429")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorTool20250429 {
    pub name: TextEditorTool20250429Name,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250429Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250728Name {
    #[serde(rename = "str_replace_based_edit_tool")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorTool20250728Type {
    #[serde(rename = "text_editor_20250728")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorTool20250728 {
    pub name: TextEditorTool20250728Name,
    #[serde(rename = "type")]
    pub type_: TextEditorTool20250728Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Examples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20250305Name {
    #[serde(rename = "web_search")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20250305Type {
    #[serde(rename = "web_search_20250305")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchTool20250305 {
    pub name: WebSearchTool20250305Name,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20250305Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20250910Name {
    #[serde(rename = "web_fetch")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20250910Type {
    #[serde(rename = "web_fetch_20250910")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchTool20250910 {
    pub name: WebFetchTool20250910Name,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20250910Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20260209Name {
    #[serde(rename = "web_search")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20260209Type {
    #[serde(rename = "web_search_20260209")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchTool20260209 {
    pub name: WebSearchTool20260209Name,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20260209Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260209Name {
    #[serde(rename = "web_fetch")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260209Type {
    #[serde(rename = "web_fetch_20260209")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchTool20260209 {
    pub name: WebFetchTool20260209Name,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260209Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260309Name {
    #[serde(rename = "web_fetch")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260309Type {
    #[serde(rename = "web_fetch_20260309")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchTool20260309 {
    pub name: WebFetchTool20260309Name,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260309Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_cache: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20260318Name {
    #[serde(rename = "web_search")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchTool20260318Type {
    #[serde(rename = "web_search_20260318")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchTool20260318 {
    pub name: WebSearchTool20260318Name,
    #[serde(rename = "type")]
    pub type_: WebSearchTool20260318Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_inclusion: Option<ResponseInclusion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260318Name {
    #[serde(rename = "web_fetch")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchTool20260318Type {
    #[serde(rename = "web_fetch_20260318")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchTool20260318 {
    pub name: WebFetchTool20260318Name,
    #[serde(rename = "type")]
    pub type_: WebFetchTool20260318Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_inclusion: Option<ResponseInclusion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_cache: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AdvisorTool20260301 {
    pub name: AdvisorTool20260301Name,
    #[serde(rename = "type")]
    pub type_: AdvisorTool20260301Type,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caching: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchBm25Tool20251119Name {
    #[serde(rename = "tool_search_tool_bm25")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchBm25Tool20251119Type {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    Tag,
    #[serde(rename = "tool_search_tool_bm25")]
    Unversioned,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolSearchBm25Tool20251119 {
    pub name: ToolSearchBm25Tool20251119Name,
    #[serde(rename = "type")]
    pub type_: ToolSearchBm25Tool20251119Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchRegexTool20251119Name {
    #[serde(rename = "tool_search_tool_regex")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchRegexTool20251119Type {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    Tag,
    #[serde(rename = "tool_search_tool_regex")]
    Unversioned,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolSearchRegexTool20251119 {
    pub name: ToolSearchRegexTool20251119Name,
    #[serde(rename = "type")]
    pub type_: ToolSearchRegexTool20251119Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseInclusion {
    Full,
    Excluded,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct UserLocation {
    #[serde(rename = "type")]
    pub type_: UserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum UserLocationType {
    Approximate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AdvisorTool20260301Name {
    #[serde(rename = "advisor")]
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AdvisorTool20260301Type {
    #[serde(rename = "advisor_20260301")]
    Tag,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpListedTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: JsonSchema,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum BrowserToolset20260801Type {
    #[serde(rename = "browser_toolset_20260801")]
    Tag,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserToolset20260801 {
    #[serde(rename = "type")]
    pub type_: BrowserToolset20260801Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<BrowserToolsetConfigs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<AllowedCaller>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserToolsetConfigs {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_tab: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_upload: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub find: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_input: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_page_text: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_key: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub javascript_exec: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_click_drag: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_mouse_down: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_mouse_up: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_tabs: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_move: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigate: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_tab: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_console: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_network: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_page: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_to: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_tab: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triple_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ToolsetMemberConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerToolset20260801Type {
    #[serde(rename = "computer_toolset_20260801")]
    Tag,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerToolset20260801 {
    #[serde(rename = "type")]
    pub type_: ComputerToolset20260801Type,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<ComputerToolsetConfigs>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerToolsetConfigs {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_position: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub double_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_key: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_click_drag: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_mouse_down: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_mouse_up: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_move: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triple_click: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<ToolsetMemberConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<ToolsetMemberConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolsetMemberConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
