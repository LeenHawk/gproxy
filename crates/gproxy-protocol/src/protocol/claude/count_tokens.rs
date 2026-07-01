use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{
    AnthropicBetaHeaders, CacheControl, ClaudeModel, ContextManagementConfig, DiagnosticsParam,
    JsonObject, JsonSchemaFormat, McpServer, MessageParam, OutputConfig, RequestServiceTier, Speed,
    SystemPrompt, ThinkingConfig, Tool, ToolChoice,
};

pub type CountTokensRequestHeaders = AnthropicBetaHeaders;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensRequestBody {
    pub model: ClaudeModel,
    pub messages: Vec<MessageParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<ContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServer>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Deprecated. Use `output_config.format` instead.
    #[deprecated(note = "use output_config.format instead")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<JsonSchemaFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<RequestServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensResponseBody {
    pub input_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<CountTokensContextManagement>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountTokensContextManagement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_input_tokens: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
