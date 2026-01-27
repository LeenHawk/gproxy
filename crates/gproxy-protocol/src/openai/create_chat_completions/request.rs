use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::openai::create_chat_completions::types::{
    ChatCompletionFunctionCallChoice, ChatCompletionFunctions, ChatCompletionRequestAudio,
    ChatCompletionRequestMessage, ChatCompletionResponseFormat, ChatCompletionStreamOptions,
    ChatCompletionToolChoiceOption, ChatCompletionToolDefinition, LogitBias, Metadata,
    PredictionContent, PromptCacheRetention, ReasoningEffort, ResponseModality, ServiceTier,
    Verbosity, WebSearchOptions,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateChatCompletionRequestBody {
    /// A list of messages comprising the conversation so far.
    /// Must contain at least 1 message (not enforced here).
    pub messages: Vec<ChatCompletionRequestMessage>,
    /// Model ID used to generate the response.
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<ResponseModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Total prompt tokens plus `max_completion_tokens` must fit the model context (not enforced here).
    pub max_completion_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Range is -2.0..=2.0 (not enforced here).
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Range is -2.0..=2.0 (not enforced here).
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_options: Option<WebSearchOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Range is 0..=20 and requires `logprobs = true` (not enforced here).
    pub top_logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ChatCompletionResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Required when requesting audio in `modalities` (not enforced here).
    pub audio: Option<ChatCompletionRequestAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<LogitBias>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Must be true when `top_logprobs` is set (not enforced here).
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Deprecated; total prompt tokens plus `max_tokens` must fit the model context (not enforced here).
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Must be at least 1; the service may enforce an upper bound (not enforced here).
    pub n: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction: Option<PredictionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Only valid when `stream` is true (not enforced here).
    pub stream_options: Option<ChatCompletionStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatCompletionToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatCompletionToolChoiceOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<ChatCompletionFunctionCallChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Deprecated; maximum of 128 entries (not enforced here).
    pub functions: Option<Vec<ChatCompletionFunctions>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Provider-specific extensions for OpenAI-compatible endpoints.
    /// This is forwarded to adapters and parsed on a best-effort basis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Range is 0..=2.0; generally avoid setting both temperature and top_p (not enforced here).
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Range is 0.0..=1.0; generally avoid setting both top_p and temperature (not enforced here).
    pub top_p: Option<f64>,
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
pub struct CreateChatCompletionRequest {
    pub body: CreateChatCompletionRequestBody,
}

/// Up to 4 stop sequences are allowed, but this limit is not enforced here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StopConfiguration {
    Single(String),
    Many(Vec<String>),
}
