use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::claude::count_tokens::types::{
    AnthropicBeta, AnthropicVersion, BetaCacheControlEphemeral, BetaContextManagementConfig,
    BetaMessageParam, BetaOutputConfig, BetaRequestMcpServerUrlDefinition, BetaSystemPrompt,
    BetaThinkingConfigParam, BetaToolChoice, BetaToolUnion, HttpMethod, Model,
};

/// Request descriptor for Claude "Count Tokens" endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeCountTokensRequest {
    /// HTTP method.
    pub method: HttpMethod,
    /// Path parameters.
    pub path: PathParameters,
    /// Query parameters.
    pub query: QueryParameters,
    /// Request headers.
    pub headers: RequestHeaders,
    /// Request body.
    pub body: RequestBody,
}

impl Default for ClaudeCountTokensRequest {
    fn default() -> Self {
        Self {
            method: HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody::default(),
        }
    }
}

/// Count tokens endpoint does not define path params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PathParameters {}

/// Count tokens endpoint does not define query params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QueryParameters {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestHeaders {
    /// Anthropic API version.
    #[serde(rename = "anthropic-version")]
    pub anthropic_version: AnthropicVersion,
    /// Optional beta version(s).
    #[serde(
        rename = "anthropic-beta",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestBody {
    /// Input messages.
    pub messages: Vec<BetaMessageParam>,
    /// Target model identifier.
    pub model: Model,
    /// Optional context management rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    /// Optional MCP servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMcpServerUrlDefinition>>,
    /// Optional top-level cache control for automatic prompt caching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    /// Optional output configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    /// Optional speed mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<BetaSystemPrompt>,
    /// Optional thinking configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    /// Optional tool choice policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    /// Optional tool definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,
}

impl Default for RequestBody {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            model: Model::Custom(String::new()),
            context_management: None,
            mcp_servers: None,
            cache_control: None,
            output_config: None,
            speed: None,
            system: None,
            thinking: None,
            tool_choice: None,
            tools: None,
        }
    }
}
