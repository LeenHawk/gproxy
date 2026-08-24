use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::{
    ChatAudioParam, ChatAudioRef, ChatFileRef, ChatWebSearchOptions, CustomToolCall, ImageUrl,
    InputAudio, PredictionContent, StreamOptions,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub messages: Vec<ChatCompletionMessageParam>,
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatAudioParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<LegacyFunctionCallChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<LegacyFunctionDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<LogitBias>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<TextOrAudioModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ModerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction: Option<PredictionContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ChatResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StringOrList>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_options: Option<ChatWebSearchOptions>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatCompletionMessageParam {
    Developer(ChatDeveloperMessageParam),
    System(ChatSystemMessageParam),
    User(ChatUserMessageParam),
    Assistant(ChatAssistantMessageParam),
    Tool(ChatToolMessageParam),
    Function(ChatFunctionMessageParam),
    Unknown(Value),
}

macro_rules! text_message {
    ($name:ident, $role:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub role: $role,
            pub content: ChatTextContent,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub name: Option<String>,
            #[serde(default, flatten)]
            pub rest: Rest,
        }
    };
}

text_message!(ChatDeveloperMessageParam, ChatDeveloperRole);
text_message!(ChatSystemMessageParam, ChatSystemRole);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatUserMessageParam {
    pub role: ChatUserRole,
    pub content: ChatContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatAssistantMessageParam {
    pub role: ChatAssistantRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ChatAssistantContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatAudioRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatToolMessageParam {
    pub role: ChatToolRole,
    pub content: ChatTextContent,
    pub tool_call_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionMessageParam {
    pub role: ChatFunctionRole,
    pub content: Option<String>,
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

macro_rules! role {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

role!(ChatDeveloperRole, Developer, "developer");
role!(ChatSystemRole, System, "system");
role!(ChatUserRole, User, "user");
role!(ChatAssistantRole, Assistant, "assistant");
role!(ChatToolRole, Tool, "tool");
role!(ChatFunctionRole, Function, "function");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatTextContent {
    Text(String),
    Parts(Vec<ChatTextContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatTextContentPart {
    Text(ChatTextPart),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTextPart {
    #[serde(rename = "type")]
    pub type_: ChatTextPartType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

role!(ChatTextPartType, Text, "text");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatAssistantContent {
    Text(String),
    Parts(Vec<ChatAssistantContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatAssistantContentPart {
    Text(ChatTextPart),
    Refusal(ChatRefusalPart),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRefusalPart {
    #[serde(rename = "type")]
    pub type_: ChatRefusalPartType,
    pub refusal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

role!(ChatRefusalPartType, Refusal, "refusal");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatContentPart {
    Text(ChatTextPart),
    ImageUrl(ChatImageUrlPart),
    InputAudio(ChatInputAudioPart),
    File(ChatFilePart),
    Unknown(Value),
}

macro_rules! input_part {
    ($name:ident, $type_name:ident, $variant:ident, $wire:literal, $field:ident, $ty:ty) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            #[serde(rename = "type")]
            pub type_: $type_name,
            pub $field: $ty,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
            #[serde(default, flatten)]
            pub rest: Rest,
        }
        role!($type_name, $variant, $wire);
    };
}

input_part!(
    ChatImageUrlPart,
    ChatImageUrlPartType,
    ImageUrl,
    "image_url",
    image_url,
    ImageUrl
);
input_part!(
    ChatInputAudioPart,
    ChatInputAudioPartType,
    InputAudio,
    "input_audio",
    input_audio,
    InputAudio
);
input_part!(
    ChatFilePart,
    ChatFilePartType,
    File,
    "file",
    file,
    ChatFileRef
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatTool {
    Function(ChatFunctionTool),
    Custom(ChatCustomTool),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionTool {
    #[serde(rename = "type")]
    pub type_: FunctionToolChoiceType,
    pub function: FunctionDefinition,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCustomTool {
    #[serde(rename = "type")]
    pub type_: CustomToolChoiceType,
    pub custom: CustomToolDefinition,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ChatToolCall {
    Function(ChatFunctionToolCall),
    Custom(ChatCustomToolCall),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatFunctionToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: FunctionToolChoiceType,
    pub function: FunctionCall,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCustomToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: CustomToolChoiceType,
    pub custom: CustomToolCall,
    #[serde(default, flatten)]
    pub rest: Rest,
}
