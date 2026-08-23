use super::common::{AnthropicBetaHeaders, ClaudeModel, ModelObjectType};
use serde::{Deserialize, Serialize};

pub type ListModelsRequestHeaders = AnthropicBetaHeaders;
pub type RetrieveModelRequestHeaders = AnthropicBetaHeaders;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListModelsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveModelPath {
    pub model_id: ClaudeModel,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListModelsResponse {
    pub data: Vec<ModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: ClaudeModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_fallback_models: Option<Vec<ClaudeModel>>,
    #[serde(rename = "type")]
    pub type_: ModelObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub batch: CapabilitySupport,
    pub citations: CapabilitySupport,
    pub code_execution: CapabilitySupport,
    pub context_management: ContextManagementCapability,
    pub effort: EffortCapability,
    pub image_input: CapabilitySupport,
    pub pdf_input: CapabilitySupport,
    pub structured_outputs: CapabilitySupport,
    pub thinking: ThinkingCapability,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySupport {
    pub supported: bool,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextManagementCapability {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_thinking_20251015: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_tool_uses_20250919: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compact_20260112: Option<CapabilitySupport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffortCapability {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medium: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xhigh: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<CapabilitySupport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingCapability {
    pub supported: bool,
    pub types: ThinkingTypes,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingTypes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<CapabilitySupport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<CapabilitySupport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelError {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
