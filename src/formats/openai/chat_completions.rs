use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;
use std::collections::HashMap;

use super::types::{
    AllowedToolDefinition, JsonSchema, JsonValue, PromptCacheRetention, ReasoningEffort,
    ServiceTier, Verbosity,
};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
#[validate(custom = validate_chat_completions_request)]
pub struct CreateChatCompletionRequest {
    #[validate(min_items = 1)]
    #[validate]
    pub messages: Vec<ChatCompletionRequestMessage>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<ChatCompletionModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = -2.0)]
    #[validate(maximum = 2.0)]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = -2.0)]
    #[validate(maximum = 2.0)]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_options: Option<WebSearchOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0)]
    #[validate(maximum = 20)]
    pub top_logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatCompletionAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub stop: Option<StopConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<HashMap<String, i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "n")]
    #[validate(minimum = 1)]
    #[validate(maximum = 128)]
    pub n: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub prediction: Option<PredictionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub stream_options: Option<ChatCompletionStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub tools: Option<Vec<ChatCompletionTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub tool_choice: Option<ChatCompletionToolChoiceOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub function_call: Option<ChatCompletionFunctionCallOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(min_items = 1)]
    #[validate(max_items = 128)]
    #[validate]
    pub functions: Option<Vec<ChatCompletionFunction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "extra_body")]
    pub extra_body: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 2.0)]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 1.0)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "role", rename_all = "lowercase")]
#[validate(custom = validate_chat_completion_message)]
pub enum ChatCompletionRequestMessage {
    Developer {
        #[validate]
        content: ChatCompletionTextContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_optional_name)]
        name: Option<String>,
    },
    System {
        #[validate]
        content: ChatCompletionTextContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_optional_name)]
        name: Option<String>,
    },
    User {
        #[validate]
        content: ChatCompletionUserContent,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_optional_name)]
        name: Option<String>,
    },
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate]
        content: Option<ChatCompletionAssistantContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        refusal: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_optional_name)]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        audio: Option<ChatCompletionAssistantAudioRef>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(min_items = 1)]
        #[validate]
        tool_calls: Option<ChatCompletionMessageToolCalls>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate]
        function_call: Option<ChatCompletionFunctionCall>,
    },
    Tool {
        #[validate]
        content: ChatCompletionTextContent,
        #[validate(min_length = 1)]
        tool_call_id: String,
    },
    Function {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[validate(min_length = 1)]
        #[validate(max_length = 64)]
        #[validate(custom = validate_name)]
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum ChatCompletionTextContent {
    Text(String),
    Parts(#[validate(min_items = 1)] Vec<ChatCompletionRequestMessageContentPartText>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum ChatCompletionUserContent {
    Text(String),
    Parts(
        #[validate(min_items = 1)]
        #[validate]
        Vec<ChatCompletionRequestUserContentPart>,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
#[validate(custom = validate_assistant_content)]
pub enum ChatCompletionAssistantContent {
    Text(String),
    Parts(Vec<ChatCompletionRequestAssistantContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionRequestUserContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ChatCompletionImageUrl,
    },
    InputAudio {
        input_audio: ChatCompletionInputAudio,
    },
    File {
        #[validate]
        file: ChatCompletionFileContent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionRequestAssistantContentPart {
    Text { text: String },
    Refusal { refusal: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequestMessageContentPartText {
    #[serde(rename = "type")]
    pub part_type: ChatCompletionTextPartType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionTextPartType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionInputAudio {
    pub data: String,
    pub format: ChatCompletionInputAudioFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionInputAudioFormat {
    Wav,
    Mp3,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(custom = validate_file_content)]
pub struct ChatCompletionFileContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ChatCompletionImageDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionImageDetail {
    Auto,
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionModality {
    Text,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<WebSearchUserLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<WebSearchContextSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchUserLocation {
    #[serde(rename = "type")]
    pub location_type: WebSearchLocationType,
    pub approximate: WebSearchLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSearchLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchContextSize {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonSchema {
        #[validate]
        json_schema: ResponseFormatJsonSchema,
    },
    JsonObject,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResponseFormatJsonSchema {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionAudio {
    pub voice: ChatCompletionAudioVoice,
    pub format: ChatCompletionAudioFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionAudioVoice {
    Alloy,
    Ash,
    Ballad,
    Coral,
    Echo,
    Fable,
    Nova,
    Onyx,
    Sage,
    Shimmer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionAudioFormat {
    Wav,
    Aac,
    Mp3,
    Flac,
    Opus,
    Pcm16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum StopConfiguration {
    Single(String),
    Multiple(
        #[validate(min_items = 1)]
        #[validate(max_items = 4)]
        Vec<String>,
    ),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PredictionContent {
    #[serde(rename = "type")]
    pub prediction_type: PredictionContentType,
    #[validate]
    pub content: PredictionContentValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionContentType {
    #[serde(rename = "content")]
    Content,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum PredictionContentValue {
    Text(String),
    Parts(#[validate(min_items = 1)] Vec<ChatCompletionRequestMessageContentPartText>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionTool {
    Function {
        #[validate]
        function: FunctionObject,
    },
    Custom {
        #[validate]
        custom: CustomToolDefinition,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct FunctionObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomToolDefinition {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CustomToolFormat {
    Text,
    Grammar { grammar: CustomToolGrammar },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomToolGrammar {
    pub definition: String,
    pub syntax: CustomToolGrammarSyntax,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomToolGrammarSyntax {
    Lark,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum ChatCompletionToolChoiceOption {
    Mode(ChatCompletionToolChoiceMode),
    AllowedTools(#[validate] ChatCompletionAllowedToolsChoice),
    NamedTool(#[validate] ChatCompletionNamedToolChoice),
    NamedCustom(#[validate] ChatCompletionNamedToolChoiceCustom),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionAllowedToolsChoice {
    #[serde(rename = "type")]
    pub choice_type: ChatCompletionAllowedToolsChoiceType,
    #[validate]
    pub allowed_tools: ChatCompletionAllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionAllowedToolsChoiceType {
    #[serde(rename = "allowed_tools")]
    AllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionAllowedTools {
    pub mode: ChatCompletionAllowedToolsMode,
    #[validate(min_items = 1)]
    pub tools: Vec<AllowedToolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionAllowedToolsMode {
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionNamedToolChoice {
    #[serde(rename = "type")]
    pub choice_type: ChatCompletionNamedToolChoiceType,
    #[validate]
    pub function: ChatCompletionNamedToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionNamedToolChoiceType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionNamedToolFunction {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionNamedToolChoiceCustom {
    #[serde(rename = "type")]
    pub choice_type: ChatCompletionNamedToolChoiceCustomType,
    #[validate]
    pub custom: ChatCompletionNamedCustomTool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionNamedToolChoiceCustomType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionNamedCustomTool {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(untagged)]
pub enum ChatCompletionFunctionCallOption {
    Mode(ChatCompletionFunctionCallMode),
    Named(#[validate] ChatCompletionFunctionCallNamed),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionFunctionCallMode {
    None,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionFunctionCallNamed {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatCompletionResponse {
    pub id: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub created: i64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    #[serde(rename = "object")]
    pub object_type: ChatCompletionObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompletionUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChoice {
    pub finish_reason: ChatCompletionFinishReason,
    pub index: i64,
    pub message: ChatCompletionResponseMessage,
    pub logprobs: Option<ChatCompletionLogprobs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatCompletionFinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponseMessage {
    pub content: Option<String>,
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<ChatCompletionMessageToolCalls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<ChatCompletionAnnotation>>,
    pub role: ChatCompletionResponseRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<ChatCompletionFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatCompletionResponseAudio>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionResponseRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponseAudio {
    pub id: String,
    pub expires_at: i64,
    pub data: String,
    pub transcript: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionAnnotation {
    UrlCitation {
        url_citation: ChatCompletionUrlCitation,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionUrlCitation {
    pub end_index: i64,
    pub start_index: i64,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionFunctionCall {
    pub arguments: String,
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
}

pub type ChatCompletionMessageToolCalls = Vec<ChatCompletionMessageToolCall>;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCompletionMessageToolCall {
    Function {
        #[validate(min_length = 1)]
        id: String,
        #[validate]
        function: ChatCompletionToolCallFunction,
    },
    Custom {
        #[validate(min_length = 1)]
        id: String,
        #[validate]
        custom: ChatCompletionCustomToolCall,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionToolCallFunction {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChatCompletionCustomToolCall {
    #[validate(min_length = 1)]
    #[validate(max_length = 64)]
    #[validate(custom = validate_name)]
    pub name: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionAssistantAudioRef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionLogprobs {
    pub content: Option<Vec<ChatCompletionTokenLogprob>>,
    pub refusal: Option<Vec<ChatCompletionTokenLogprob>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionTokenLogprob {
    pub token: String,
    pub logprob: f64,
    pub bytes: Option<Vec<i64>>,
    pub top_logprobs: Vec<ChatCompletionTopLogprob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionTopLogprob {
    pub token: String,
    pub logprob: f64,
    pub bytes: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionUsage {
    pub completion_tokens: i64,
    pub prompt_tokens: i64,
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_properties: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_prediction_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_prediction_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChatCompletionStreamResponse {
    pub id: String,
    pub choices: Vec<ChatCompletionChunkChoice>,
    pub created: i64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    #[serde(rename = "object")]
    pub object_type: ChatCompletionChunkObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompletionUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunkChoice {
    pub delta: ChatCompletionStreamResponseDelta,
    pub logprobs: Option<ChatCompletionLogprobs>,
    pub finish_reason: Option<ChatCompletionFinishReason>,
    pub index: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionStreamResponseDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<ChatCompletionFunctionCallDelta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatCompletionMessageToolCallChunk>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<ChatCompletionStreamRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obfuscation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionMessageToolCallChunk {
    pub index: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<ChatCompletionToolCallType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ChatCompletionToolCallFunctionChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionToolCallType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionToolCallFunctionChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionFunctionCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionStreamRole {
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionObjectType {
    #[serde(rename = "chat.completion")]
    ChatCompletion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatCompletionChunkObjectType {
    #[serde(rename = "chat.completion.chunk")]
    ChatCompletionChunk,
}

fn validate_chat_completions_request(
    req: &CreateChatCompletionRequest,
) -> Result<(), ValidationError> {
    if req.top_logprobs.is_some() && req.logprobs != Some(true) {
        return Err(ValidationError::Custom(
            "top_logprobs requires logprobs=true".to_string(),
        ));
    }

    if req.stream_options.is_some() && req.stream != Some(true) {
        return Err(ValidationError::Custom(
            "stream_options requires stream=true".to_string(),
        ));
    }

    if let Some(modalities) = &req.modalities {
        let wants_audio = modalities
            .iter()
            .any(|modality| matches!(modality, ChatCompletionModality::Audio));
        if wants_audio && req.audio.is_none() {
            return Err(ValidationError::Custom(
                "audio is required when modalities includes audio".to_string(),
            ));
        }
    }

    if let Some(logit_bias) = &req.logit_bias {
        for (token, value) in logit_bias {
            if *value < -100 || *value > 100 {
                return Err(ValidationError::Custom(format!(
                    "logit_bias[{token}] must be between -100 and 100"
                )));
            }
        }
    }

    if let Some(metadata) = &req.metadata {
        if metadata.len() > 16 {
            return Err(ValidationError::Custom(
                "metadata must not contain more than 16 entries".to_string(),
            ));
        }
        for (key, value) in metadata {
            if key.len() > 64 {
                return Err(ValidationError::Custom(
                    "metadata keys must be at most 64 characters".to_string(),
                ));
            }
            if value.len() > 512 {
                return Err(ValidationError::Custom(
                    "metadata values must be at most 512 characters".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_chat_completion_message(
    message: &ChatCompletionRequestMessage,
) -> Result<(), ValidationError> {
    if let ChatCompletionRequestMessage::Assistant {
        content,
        tool_calls,
        function_call,
        ..
    } = message
    {
        if content.is_none() && tool_calls.is_none() && function_call.is_none() {
            return Err(ValidationError::Custom(
                "assistant content is required unless tool_calls or function_call is set"
                    .to_string(),
            ));
        }
        if tool_calls.is_some() && function_call.is_some() {
            return Err(ValidationError::Custom(
                "assistant tool_calls and function_call are mutually exclusive".to_string(),
            ));
        }
    }

    Ok(())
}

fn validate_assistant_content(
    content: &ChatCompletionAssistantContent,
) -> Result<(), ValidationError> {
    if let ChatCompletionAssistantContent::Parts(parts) = content {
        if parts.is_empty() {
            return Err(ValidationError::Custom(
                "assistant content parts must not be empty".to_string(),
            ));
        }
        let refusal_count = parts
            .iter()
            .filter(|part| {
                matches!(
                    part,
                    ChatCompletionRequestAssistantContentPart::Refusal { .. }
                )
            })
            .count();
        if refusal_count > 0 && parts.len() != 1 {
            return Err(ValidationError::Custom(
                "assistant content refusal must be the only part".to_string(),
            ));
        }
    }

    Ok(())
}

fn validate_optional_name(name: &Option<String>) -> Result<(), ValidationError> {
    if let Some(name) = name {
        validate_name(name)?;
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ValidationError> {
    if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Ok(())
    } else {
        Err(ValidationError::Custom(
            "name must match [A-Za-z0-9_-] only".to_string(),
        ))
    }
}

fn validate_file_content(file: &ChatCompletionFileContent) -> Result<(), ValidationError> {
    let has_file_id = file.file_id.is_some();
    let has_file_data = file.file_data.is_some();
    if has_file_id == has_file_data {
        return Err(ValidationError::Custom(
            "file must include exactly one of file_id or file_data".to_string(),
        ));
    }
    if has_file_data && file.filename.as_deref().unwrap_or("").is_empty() {
        return Err(ValidationError::Custom(
            "file.filename is required when file_data is set".to_string(),
        ));
    }
    Ok(())
}
