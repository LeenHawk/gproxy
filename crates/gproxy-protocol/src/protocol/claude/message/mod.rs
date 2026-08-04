use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{
    AnthropicBetaHeaders, AssistantRole, CacheControl, ClaudeModel, Container, ContainerParam,
    ContentBlock, ContextManagementConfig, ContextManagementResponse, Diagnostics,
    DiagnosticsParam, FallbackCreditTokenParam, FallbacksParam, InferenceGeo, JsonObject,
    JsonSchemaFormat, McpServer, MessageObjectType, MessageParam, Metadata, OutputConfig,
    RequestServiceTier, Speed, StopDetails, StopReason, SystemPrompt, ThinkingConfig, Tool,
    ToolChoice, Usage,
};

pub mod stream;

pub use stream::*;

pub type CreateMessageRequestHeaders = AnthropicBetaHeaders;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateMessageRequestBody {
    pub model: ClaudeModel,
    pub messages: Vec<MessageParam>,
    pub max_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<ContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<DiagnosticsParam>,
    /// Redeem a prior refusal's fallback credit on retry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_credit_token: Option<FallbackCreditTokenParam>,
    /// Server-side retry routing used when the requested model refuses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<FallbacksParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<InferenceGeo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServer>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
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
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_profile_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateMessageResponseBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: MessageObjectType,
    pub role: AssistantRole,
    pub content: Vec<ContentBlock>,
    pub model: ClaudeModel,
    pub stop_reason: StopReason,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<ContextManagementResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Diagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<StopDetails>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::claude::{
        ContentBlockParam, MidConversationSystemContentBlock, StringOrArray, WebFetchTool,
        WebSearchTool,
    };

    #[test]
    fn parses_default_fallback_and_mid_conversation_tool_changes() {
        let request: CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-opus-5",
            "messages": [{
                "role": "user",
                "content": [{
                    "type": "mid_conv_system",
                    "content": [
                        {"type": "text", "text": "Use the newly available tool."},
                        {"type": "tool_addition", "tool": {"type": "tool_reference", "name": "search"}},
                        {"type": "tool_removal", "tool": {"type": "mcp_toolset_reference", "server_name": "legacy"}}
                    ]
                }]
            }],
            "max_tokens": 1024,
            "fallbacks": "default"
        }))
        .unwrap();

        assert!(matches!(
            request.model,
            ClaudeModel::Known(super::super::common::ClaudeModelKnown::ClaudeOpus5)
        ));
        assert!(matches!(
            request.fallbacks,
            Some(FallbacksParam::Default(_))
        ));
        let StringOrArray::Array(blocks) = &request.messages[0].content else {
            panic!("expected content blocks");
        };
        let ContentBlockParam::MidConversationSystem(system) = &blocks[0] else {
            panic!("expected mid-conversation system block");
        };
        assert!(matches!(
            system.content[1],
            MidConversationSystemContentBlock::ToolAddition(_)
        ));
        assert!(matches!(
            system.content[2],
            MidConversationSystemContentBlock::ToolRemoval(_)
        ));
    }

    #[test]
    fn parses_ordered_fallbacks_and_latest_web_tools() {
        let request: CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-fable-5",
            "messages": [{"role": "user", "content": "hello"}],
            "max_tokens": 1024,
            "fallbacks": [
                {"model": "claude-opus-5"},
                {"model": "claude-opus-4-8"},
                {"model": "claude-sonnet-5"}
            ],
            "tools": [
                {"type": "web_search_20260318", "name": "web_search", "response_inclusion": "excluded"},
                {"type": "web_fetch_20260318", "name": "web_fetch", "response_inclusion": "full", "use_cache": false}
            ]
        }))
        .unwrap();

        let Some(FallbacksParam::Models(fallbacks)) = request.fallbacks else {
            panic!("expected explicit fallback chain");
        };
        assert_eq!(fallbacks.len(), 3);
        let tools = request.tools.unwrap();
        assert!(matches!(
            tools[0],
            Tool::WebSearch(WebSearchTool::WebSearch20260318(_))
        ));
        assert!(matches!(
            tools[1],
            Tool::WebFetch(WebFetchTool::WebFetch20260318(_))
        ));
    }
}
