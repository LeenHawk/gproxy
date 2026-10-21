use crate::Rest;
use serde::{Deserialize, Serialize};

pub(crate) fn present_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub(crate) fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
// Keep the public wire variants inline, as with the Claude content unions.
#[allow(clippy::large_enum_variant)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ChatMessage {
    Developer(DeveloperMessage),
    System(SystemMessage),
    User(UserMessage),
    Assistant(AssistantMessage),
    Tool(ToolMessage),
    Function(FunctionMessage),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DeveloperMessage {
    pub role: DeveloperRole,
    pub content: TextContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum DeveloperRole {
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SystemMessage {
    pub role: SystemRole,
    pub content: TextContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SystemRole {
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UserMessage {
    pub role: UserRole,
    pub content: UserContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UserRole {
    #[serde(rename = "user")]
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssistantMessage {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_details: Option<Option<Vec<super::reasoning::ReasoningDetail>>>,
    /// Plain-text reasoning returned by compatible providers (for example DeepSeek).
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_content: Option<Option<String>>,
    /// Alternative compatible-provider spelling. Kept separately on the wire.
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<Option<String>>,
    pub role: AssistantRole,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Option<AssistantContent>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio: Option<Option<AssistantAudio>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub refusal: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<MessageToolCall>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub function_call: Option<Option<FunctionCall>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AssistantRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolMessage {
    pub role: ToolRole,
    pub content: TextContent,
    pub tool_call_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ToolRole {
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionMessage {
    pub role: FunctionRole,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "required_nullable")]
    pub content: Option<String>,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FunctionRole {
    #[serde(rename = "function")]
    Function,
}

pub type TextContent = StringOrTextParts;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum StringOrTextParts {
    Text(String),
    Parts(Vec<TextPart>),
}
pub type UserContent = UserContentParts;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UserContentParts {
    Text(String),
    Parts(Vec<UserContentPart>),
}
pub type AssistantContent = AssistantContentParts;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AssistantContentParts {
    Text(String),
    Parts(Vec<AssistantContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UserContentPart {
    #[serde(rename = "text")]
    Text(TextPart),
    #[serde(rename = "image_url")]
    Image(ImagePart),
    #[serde(rename = "input_audio")]
    InputAudio(InputAudioPart),
    #[serde(rename = "file")]
    File(FilePart),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AssistantContentPart {
    #[serde(rename = "text")]
    Text(TextPart),
    #[serde(rename = "refusal")]
    Refusal(RefusalPart),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TextPart {
    #[serde(rename = "type")]
    pub type_: TextPartType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TextPartType {
    #[serde(rename = "text")]
    Text,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RefusalPart {
    #[serde(rename = "type")]
    pub type_: RefusalType,
    pub refusal: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum RefusalType {
    #[serde(rename = "refusal")]
    Refusal,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImagePart {
    #[serde(rename = "type")]
    pub type_: ImagePartType,
    pub image_url: ImageUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ImagePartType {
    #[serde(rename = "image_url")]
    ImageUrl,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ImageDetail {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "high")]
    High,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InputAudioPart {
    #[serde(rename = "type")]
    pub type_: InputAudioPartType,
    pub input_audio: InputAudio,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum InputAudioPartType {
    #[serde(rename = "input_audio")]
    InputAudio,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AudioFormat {
    #[serde(rename = "wav")]
    Wav,
    #[serde(rename = "mp3")]
    Mp3,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InputAudio {
    pub data: String,
    pub format: AudioFormat,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FilePart {
    #[serde(rename = "type")]
    pub type_: FilePartType,
    pub file: FileRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FilePartType {
    #[serde(rename = "file")]
    File,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FileRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PromptCacheBreakpoint {
    pub mode: PromptCacheMode,
    /// Minimum lifetime of this breakpoint; omitted inherits the request TTL.
    /// OpenAI OpenAPI: PromptCacheBreakpointParam (2026-10-21).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<PromptCacheBreakpointTtl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Explicit cache-breakpoint lifetimes from OpenAI's PromptCacheBreakpointParam.
/// Separate from the request-wide PromptCacheTTLEnum and legacy retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PromptCacheBreakpointTtl {
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "12h")]
    TwelveHours,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum PromptCacheMode {
    #[serde(rename = "explicit")]
    Explicit,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssistantAudio {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionCall {
    pub arguments: String,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChatToolCall {
    pub id: String,
    pub function: FunctionCall,
    #[serde(rename = "type")]
    pub type_: ChatToolCallType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CustomToolCall {
    pub id: String,
    pub custom: CustomCall,
    #[serde(rename = "type")]
    pub type_: CustomToolCallType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CustomCall {
    pub input: String,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ChatToolCallType {
    #[serde(rename = "function")]
    Function,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CustomToolCallType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MessageToolCall {
    Function(ChatToolCall),
    Custom(CustomToolCall),
}
