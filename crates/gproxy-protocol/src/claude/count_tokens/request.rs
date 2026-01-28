use serde::{Deserialize, Serialize};

use crate::claude::count_tokens::types::Model;
use crate::claude::types::{
    AnthropicHeaders, BetaContextManagementConfig, BetaJSONOutputFormat, BetaMessageParam,
    BetaOutputConfig, BetaRequestMCPServerURLDefinition, BetaSystemParam, BetaThinkingConfigParam,
    BetaTool, BetaToolChoice,
};

pub type CountTokensHeaders = AnthropicHeaders;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensRequestBody {
    pub messages: Vec<BetaMessageParam>,
    pub model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<BetaSystemParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    /// Requires the `structured-outputs-2025-11-13` beta header.
    /// Structured outputs are currently available as a public beta feature in the Claude API for
    /// Claude Sonnet 4.5, Claude Opus 4.1, Claude Opus 4.5, and Claude Haiku 4.5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<BetaJSONOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMCPServerURLDefinition>>,
}

#[derive(Debug, Clone)]
pub struct CountTokensRequest {
    pub headers: CountTokensHeaders,
    pub body: CountTokensRequestBody,
}
