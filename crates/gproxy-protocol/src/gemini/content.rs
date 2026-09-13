use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Modality {
    #[serde(rename = "MODALITY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "TEXT")]
    Text,
    #[serde(rename = "IMAGE")]
    Image,
    #[serde(rename = "VIDEO")]
    Video,
    #[serde(rename = "AUDIO")]
    Audio,
    #[serde(rename = "DOCUMENT")]
    Document,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SchemaType {
    #[serde(rename = "TYPE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "STRING")]
    String,
    #[serde(rename = "NUMBER")]
    Number,
    #[serde(rename = "INTEGER")]
    Integer,
    #[serde(rename = "BOOLEAN")]
    Boolean,
    #[serde(rename = "ARRAY")]
    Array,
    #[serde(rename = "OBJECT")]
    Object,
    #[serde(rename = "NULL")]
    Null,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum HarmCategory {
    #[serde(rename = "HARM_CATEGORY_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "HARM_CATEGORY_HARASSMENT")]
    Harassment,
    #[serde(rename = "HARM_CATEGORY_HATE_SPEECH")]
    HateSpeech,
    #[serde(rename = "HARM_CATEGORY_SEXUALLY_EXPLICIT")]
    SexuallyExplicit,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS_CONTENT")]
    DangerousContent,
    #[serde(rename = "HARM_CATEGORY_CIVIC_INTEGRITY")]
    CivicIntegrity,
    #[serde(rename = "HARM_CATEGORY_DEROGATORY")]
    Derogatory,
    #[serde(rename = "HARM_CATEGORY_TOXICITY")]
    Toxicity,
    #[serde(rename = "HARM_CATEGORY_VIOLENCE")]
    Violence,
    #[serde(rename = "HARM_CATEGORY_SEXUAL")]
    Sexual,
    #[serde(rename = "HARM_CATEGORY_MEDICAL")]
    Medical,
    #[serde(rename = "HARM_CATEGORY_DANGEROUS")]
    Dangerous,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum HarmBlockThreshold {
    #[serde(rename = "HARM_BLOCK_THRESHOLD_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "BLOCK_LOW_AND_ABOVE")]
    BlockLowAndAbove,
    #[serde(rename = "BLOCK_MEDIUM_AND_ABOVE")]
    BlockMediumAndAbove,
    #[serde(rename = "BLOCK_ONLY_HIGH")]
    BlockOnlyHigh,
    #[serde(rename = "BLOCK_NONE")]
    BlockNone,
    #[serde(rename = "OFF")]
    Off,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FunctionCallingMode {
    #[serde(rename = "MODE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "AUTO")]
    Auto,
    #[serde(rename = "ANY")]
    Any,
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "VALIDATED")]
    Validated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Content {
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<MediaResolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Blob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_code: Option<ExecutableCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution_result: Option<CodeExecutionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<ToolResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_metadata: Option<VideoMetadata>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Blob {
    pub mime_type: String,
    pub data: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileData {
    pub file_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionCall {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Rest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<FunctionResponsePart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_continue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionResponsePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Blob>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ExecutableCode {
    pub language: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecutionResult {
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolCall {
    pub tool_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolResponse {
    pub tool_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct VideoMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct MediaResolution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Schema {
    #[serde(rename = "type")]
    pub type_: SchemaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<std::collections::BTreeMap<String, Box<Schema>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Box<Schema>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_ordering: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters_json_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<serde_json::Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GoogleSearchRetrieval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_retrieval_config: Option<DynamicRetrievalConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct DynamicRetrievalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_threshold: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeExecution {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GoogleSearch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range_filter: Option<Interval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_types: Option<SearchTypes>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Interval {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SearchTypes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search: Option<WebSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_search: Option<ImageSearch>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearch {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageSearch {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UrlContext {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearch {
    pub file_search_store_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpServer {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streamable_http_transport: Option<StreamableHttpTransport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct StreamableHttpTransport {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sse_read_timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate_on_close: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerUse {
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_predefined_functions: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GoogleMaps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_widget: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Tool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<FunctionDeclaration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_retrieval: Option<GoogleSearchRetrieval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution: Option<CodeExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search: Option<GoogleSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<ComputerUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_context: Option<UrlContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_search: Option<FileSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServer>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps: Option<GoogleMaps>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<FunctionCallingMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<FunctionCallingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_config: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_server_side_tool_invocations: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SafetySetting {
    pub category: HarmCategory,
    pub threshold: HarmBlockThreshold,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
