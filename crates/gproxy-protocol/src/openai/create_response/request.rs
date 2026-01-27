use serde::{Deserialize, Serialize};

use crate::openai::create_response::types::{
    ConversationParam, InputParam, Instructions, Metadata, Prompt, PromptCacheRetention, Reasoning,
    ResponseInclude, ResponseStreamOptions, ResponseTextParam, ServiceTier, Tool, ToolChoiceParam,
    Truncation,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateResponseRequestBody {
    /// Model ID used to generate the response.
    pub model: String,
    /// Text, image, or file inputs to the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<ResponseInclude>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// When used with `previous_response_id`, prior instructions are not carried over (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Instructions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Only valid when `stream` is true (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<ResponseStreamOptions>,
    /// Cannot be used together with `previous_response_id` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponseTextParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoiceParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    /// Range is 0..=20 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Range is 0..=2.0 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Range is 0..=1.0 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Deprecated; prefer `safety_identifier` or `prompt_cache_key` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
}

#[derive(Debug, Clone)]
pub struct CreateResponseRequest {
    pub body: CreateResponseRequestBody,
}
