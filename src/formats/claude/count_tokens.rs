use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensHeaders {
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensRequest {
    pub messages: MessageList,
    pub model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMCPServerURLDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<BetaJSONOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageTokensCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaCountTokensContextManagementResponse>,
    pub input_tokens: u32,
}

pub type CountTokensResponse = BetaMessageTokensCount;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCountTokensContextManagementResponse {
    pub original_input_tokens: u32,
}
