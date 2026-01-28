use serde::{Deserialize, Serialize};

use crate::claude::count_tokens::types::{
    BetaContextManagementConfig, BetaJSONOutputFormat, BetaMessageParam, BetaOutputConfig,
    BetaRequestMCPServerURLDefinition, BetaSystemParam, BetaThinkingConfigParam, BetaTool,
    BetaToolChoice, Model,
};
use crate::claude::create_message::types::{BetaContainerParam, BetaMetadata, BetaServiceTier};
use crate::claude::types::AnthropicHeaders;

pub type CreateMessageHeaders = AnthropicHeaders;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequestBody {
    /// Maximum tokens to generate; model-specific maximums apply.
    pub max_tokens: u32,
    /// Up to 100,000 messages; consecutive user/assistant turns are combined.
    pub messages: Vec<BetaMessageParam>,
    pub model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainerParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMCPServerURLDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BetaMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<BetaJSONOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<BetaServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// If true, the response is streamed as SSE events instead of a single message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<BetaSystemParam>,
    /// Range 0.0-1.0. Avoid setting both temperature and top_p.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaTool>>,
    /// Recommended for advanced use cases only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Range 0.0-1.0. Avoid setting both top_p and temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct CreateMessageRequest {
    pub headers: CreateMessageHeaders,
    pub body: CreateMessageRequestBody,
}
