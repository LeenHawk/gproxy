//! OpenAI Responses generation request body (`POST /responses`).
//! Source: Create a model response.md, Body Parameters (lines 15-5760).
use super::input::{
    ConversationParam, Input, ReasoningConfig, ResponseInputFile, ResponseInputImage,
    ResponseInputText, TextConfig, present_nullable, present_optional,
};
use super::tools::Tool;
use crate::Rest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GenerateContentRequestBody {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub background: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_management: Option<Option<Vec<ContextManagement>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub conversation: Option<Option<ConversationParam>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub include: Option<Option<Vec<ResponseIncludable>>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub input: Option<Input>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tool_calls: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub metadata: Option<Option<BTreeMap<String, String>>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub moderation: Option<Option<ResponseModeration>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub parallel_tool_calls: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_response_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt: Option<Option<ResponsePrompt>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_retention: Option<Option<PromptCacheRetention>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<Option<ReasoningConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub safety_identifier: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_tier: Option<Option<ServiceTier>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub store: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream_options: Option<Option<StreamOptions>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub temperature: Option<Option<serde_json::Number>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<TextConfig>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_choice: Option<super::input::ToolChoice>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub tools: Option<Vec<Tool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub top_logprobs: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub top_p: Option<Option<serde_json::Number>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub truncation: Option<Option<GenerateTruncation>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub user: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContextManagement {
    pub r#type: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub compact_threshold: Option<Option<i64>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum ResponseIncludable {
    #[serde(rename = "web_search_call.action.sources")]
    WebSearchSources,
    #[serde(rename = "code_interpreter_call.outputs")]
    CodeInterpreterOutputs,
    #[serde(rename = "computer_call_output.output.image_url")]
    ComputerImageUrl,
    #[serde(rename = "file_search_call.results")]
    FileSearchResults,
    #[serde(rename = "message.input_image.image_url")]
    InputImageUrl,
    #[serde(rename = "message.output_text.logprobs")]
    OutputLogprobs,
    #[serde(rename = "reasoning.encrypted_content")]
    ReasoningEncryptedContent,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseModeration {
    pub model: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub policy: Option<Option<ModerationPolicy>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum ModerationMode {
    #[serde(rename = "score")]
    Score,
    #[serde(rename = "block")]
    Block,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModerationPolicy {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input: Option<Option<ModerationPolicyMode>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output: Option<Option<ModerationPolicyMode>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ModerationPolicyMode {
    pub mode: ModerationMode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ResponsePrompt {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub variables: Option<Option<BTreeMap<String, PromptVariableValue>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub version: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum PromptCachingMode {
    #[serde(rename = "implicit")]
    Implicit,
    #[serde(rename = "explicit")]
    Explicit,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct PromptCacheOptions {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub comparison_response_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prewarm: Option<bool>,

    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub mode: Option<PromptCachingMode>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub ttl: Option<PromptCacheTtl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum PromptCacheRetention {
    #[serde(rename = "in_memory")]
    InMemory,
    #[serde(rename = "24h")]
    TwentyFourHours,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum ServiceTier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "flex")]
    Flex,
    #[serde(rename = "scale")]
    Scale,
    #[serde(rename = "priority")]
    Priority,
    #[serde(rename = "fast")]
    Fast,
    #[serde(rename = "ultrafast")]
    Ultrafast,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct StreamOptions {
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_obfuscation: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum GenerateTruncation {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "disabled")]
    Disabled,
}
pub type GenerateContentRequest = crate::WireRequest<GenerateContentRequestBody>;

/// Prompt variables are a closed union, not arbitrary JSON (source line 4419).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum PromptVariableValue {
    String(String),
    Text(ResponseInputText),
    Image(ResponseInputImage),
    File(ResponseInputFile),
}

/// `ttl` has exactly one documented value (source line 4459).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum PromptCacheTtl {
    #[serde(rename = "30m")]
    ThirtyMinutes,
}
