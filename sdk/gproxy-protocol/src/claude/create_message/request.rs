use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::claude::create_message::types::{
    AnthropicBeta, AnthropicVersion, BetaCacheControlEphemeral, BetaContainerRef,
    BetaContextManagementConfig, BetaMessageParam, BetaMetadata, BetaOutputConfig,
    BetaRequestMcpServerUrlDefinition, BetaServiceTierParam, BetaSpeed,
    BetaSystemPrompt, BetaThinkingConfigParam, BetaToolChoice, BetaToolUnion, HttpMethod, Model,
};

/// Request descriptor for Claude "Create a Message" endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeCreateMessageRequest {
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

impl Default for ClaudeCreateMessageRequest {
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

/// Create-message endpoint does not define path params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PathParameters {}

/// Create-message endpoint does not define query params.
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
    /// Maximum number of tokens to generate.
    pub max_tokens: u64,
    /// Input messages.
    pub messages: Vec<BetaMessageParam>,
    /// Target model identifier.
    pub model: Model,
    /// Optional container id or container parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainerRef>,
    /// Optional context management rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    /// Optional inference geography hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    /// Optional MCP servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMcpServerUrlDefinition>>,
    /// Optional per-request metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BetaMetadata>,
    /// Optional top-level cache control for automatic prompt caching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    /// Optional output configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    /// Optional service tier selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<BetaServiceTierParam>,
    /// Optional inference speed mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<BetaSpeed>,
    /// Optional custom stop sequences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Optional streaming toggle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Optional system prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<BetaSystemPrompt>,
    /// Optional temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Optional thinking configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    /// Optional tool choice policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    /// Optional tool definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,
    /// Optional top-k sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    /// Optional top-p sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

impl Default for RequestBody {
    fn default() -> Self {
        Self {
            max_tokens: 0,
            messages: Vec::new(),
            model: Model::Custom(String::new()),
            container: None,
            context_management: None,
            inference_geo: None,
            mcp_servers: None,
            metadata: None,
            cache_control: None,
            output_config: None,
            service_tier: None,
            speed: None,
            stop_sequences: None,
            stream: None,
            system: None,
            temperature: None,
            thinking: None,
            tool_choice: None,
            tools: None,
            top_k: None,
            top_p: None,
        }
    }
}
