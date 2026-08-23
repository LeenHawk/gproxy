use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseTool {
    #[serde(rename = "type")]
    pub type_: ToolType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<ToolCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_store_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<FileSearchRankingOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<McpAllowedTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<McpConnectorId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<McpRequireApproval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<CodeInterpreterContainer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ImageGenerationAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<ImageInputFidelity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ImageMask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageEditQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ResponseImageGenerationSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolInputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseNamespaceTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolSearchExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_content_types: Option<Vec<SearchContentType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseNamespaceTool {
    #[serde(rename = "type")]
    pub type_: ToolType,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolInputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<ToolCaller>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
