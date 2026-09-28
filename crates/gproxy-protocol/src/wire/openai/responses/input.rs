//! Native OpenAI Responses input, output items, tool and configuration shapes.
//!
//! The request uses the same native input and tool shapes as Responses.  The
//! types here deliberately remain OpenAI-shaped; they are not a shared IR.
//!
//! Shapes follow `Get input token counts.md`, including native history items.
//! Known structures are typed; `rest` retains only unknown extension fields.
use super::tools::*;
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Deserialize a present non-null optional field; a missing field uses serde default.
pub(in crate::wire::openai) fn present_optional<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// A field which is required on the wire but explicitly permits JSON null.
/// `deserialize_with` preserves the distinction between a missing field
/// (serde reports an error) and a present null (returns `None`).
pub(in crate::wire::openai) fn required_nullable<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub(in crate::wire::openai) fn present_nullable<'de, D, T>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseConversationParam {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationParam {
    Id(String),
    Object(ResponseConversationParam),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum Input {
    Text(String),
    Items(Vec<InputItem>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<InputContent>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum InputContent {
    #[serde(rename = "input_text")]
    Text(ResponseInputText),
    #[serde(rename = "input_image")]
    Image(ResponseInputImage),
    #[serde(rename = "input_file")]
    File(ResponseInputFile),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseInputText {
    #[serde(rename = "type")]
    pub type_: ResponseInputTextType,
    pub text: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseInputImage {
    #[serde(rename = "type")]
    pub type_: ResponseInputImageType,
    pub detail: ImageDetail,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub image_url: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseInputFile {
    #[serde(rename = "type")]
    pub type_: ResponseInputFileType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub detail: Option<FileDetail>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_data: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub filename: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PromptCacheBreakpoint {
    pub mode: PromptCacheMode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct EasyInputMessage {
    pub content: MessageContent,
    pub role: MessageRole,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub phase: Option<Option<MessagePhase>>,
    #[serde(
        rename = "type",
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<MessageType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct InputMessage {
    pub content: Vec<InputContent>,
    pub role: InputMessageRole,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<OutputMessageStatus>,
    #[serde(
        rename = "type",
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<MessageType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum InputItem {
    ConfigurationUpdate(ConfigurationUpdate),
    OutputMessage(#[serde(deserialize_with = "output_message_history")] ResponseOutputMessage),
    Message(InputMessage),
    Easy(EasyInputMessage),
    FunctionCall(FunctionCall),
    FunctionCallOutput(FunctionCallOutput),
    Reasoning(ReasoningItem),
    ItemReference(ItemReference),
    Compaction(Compaction),
    ComputerCall(ComputerCall),
    ComputerCallOutput(ComputerCallOutput),
    WebSearchCall(WebSearchCall),
    FileSearchCall(FileSearchCall),
    ImageGenerationCall(ImageGenerationCall),
    CodeInterpreterCall(CodeInterpreterCall),
    CustomToolCall(CustomToolCall),
    CustomToolCallOutput(CustomToolCallOutput),
    ToolSearchCall(ToolSearchCall),
    ToolSearchOutput(ToolSearchOutput),
    AdditionalTools(AdditionalTools),
    LocalShellCall(LocalShellCall),
    LocalShellCallOutput(LocalShellCallOutput),
    ShellCall(ShellCall),
    ShellCallOutput(ShellCallOutput),
    ApplyPatchCall(ApplyPatchCall),
    ApplyPatchCallOutput(ApplyPatchCallOutput),
    McpListTools(McpListTools),
    McpApprovalRequest(McpApprovalRequest),
    McpApprovalResponse(McpApprovalResponse),
    McpCall(McpCall),
    CompactionTrigger(CompactionTrigger),
    Program(Program),
    ProgramOutput(ProgramOutput),
}

/// Codex replays assistant history without output-only status/annotation
/// bookkeeping. Accept that compact request form without relaxing live
/// response parsing or dropping its output_text content.
fn output_message_history<'de, D>(deserializer: D) -> Result<ResponseOutputMessage, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut value = serde_json::Value::deserialize(deserializer)?;
    if value.get("role").and_then(serde_json::Value::as_str) == Some("assistant")
        && value.get("type").and_then(serde_json::Value::as_str) == Some("message")
    {
        let object = value.as_object_mut().expect("message object");
        object.entry("status").or_insert_with(|| "completed".into());
        if let Some(parts) = object
            .get_mut("content")
            .and_then(serde_json::Value::as_array_mut)
        {
            for part in parts {
                if part.get("type").and_then(serde_json::Value::as_str) == Some("output_text") {
                    let part = part.as_object_mut().expect("text object");
                    part.entry("annotations")
                        .or_insert_with(|| serde_json::json!([]));
                    part.entry("logprobs")
                        .or_insert_with(|| serde_json::json!([]));
                }
            }
        }
    }
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputText {
    #[serde(rename = "type")]
    pub type_: ResponseOutputTextType,
    pub text: String,
    pub annotations: Vec<OutputAnnotation>,
    pub logprobs: Vec<OutputLogprob>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum OutputAnnotation {
    File(FileCitation),
    Url(UrlCitation),
    ContainerFile(ContainerFileCitation),
    Path(FilePath),
}
macro_rules! citation { ($name:ident { $($field:tt)* }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all = "snake_case")] #[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest } }; }
citation!(FileCitation { pub file_id: String, pub filename: String, pub index: i64, #[serde(rename = "type")] pub type_: FileCitationType, });
citation!(UrlCitation { pub end_index: i64, pub start_index: i64, pub title: String, pub url: String, #[serde(rename = "type")] pub type_: UrlCitationType, });
citation!(ContainerFileCitation { pub container_id: String, pub end_index: i64, pub file_id: String, pub filename: String, pub start_index: i64, #[serde(rename = "type")] pub type_: ContainerFileCitationType, });
citation!(FilePath { pub file_id: String, pub index: i64, #[serde(rename = "type")] pub type_: FilePathType, });
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
pub enum FileCitationType {
    #[serde(rename = "file_citation")]
    FileCitation,
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
pub enum UrlCitationType {
    #[serde(rename = "url_citation")]
    UrlCitation,
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
pub enum ContainerFileCitationType {
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation,
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
pub enum FilePathType {
    #[serde(rename = "file_path")]
    FilePath,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct OutputLogprob {
    pub token: String,
    pub bytes: Vec<i64>,
    pub logprob: serde_json::Number,
    pub top_logprobs: Vec<TopLogprob>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TopLogprob {
    pub token: String,
    pub bytes: Vec<i64>,
    pub logprob: serde_json::Number,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputRefusal {
    #[serde(rename = "type")]
    pub type_: ResponseOutputRefusalType,
    pub refusal: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FunctionCall {
    #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
    pub async_: Option<bool>,

    #[serde(rename = "type")]
    pub type_: FunctionCallType,
    pub arguments: String,
    pub call_id: String,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub namespace: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<ItemStatus>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FunctionCallOutput {
    #[serde(rename = "type")]
    pub type_: FunctionCallOutputType,
    pub call_id: String,
    pub output: FunctionOutput,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub namespace: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<Caller>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<Option<ItemStatus>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FunctionOutput {
    Text(String),
    Content(Vec<FunctionOutputContent>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FunctionOutputContent {
    Text(FunctionOutputText),
    Image(FunctionOutputImage),
    File(FunctionOutputFile),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FunctionOutputImage {
    #[serde(rename = "type")]
    pub type_: ResponseInputImageType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub detail: Option<Option<ImageDetail>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub image_url: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<Option<PromptCacheBreakpoint>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FunctionOutputFile {
    #[serde(rename = "type")]
    pub type_: ResponseInputFileType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_data: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_url: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<Option<PromptCacheBreakpoint>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub detail: Option<FileDetail>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub filename: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningConfig {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub effort: Option<Option<ReasoningEffort>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub generate_summary: Option<Option<ReasoningSummary>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub mode: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub summary: Option<Option<ReasoningSummary>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub context: Option<Option<ReasoningContext>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningItem {
    #[serde(rename = "type")]
    pub type_: ReasoningItemType,
    pub id: String,
    pub summary: Vec<SummaryText>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Vec<ReasoningContent>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<ReasoningStatus>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub encrypted_content: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningContent {
    #[serde(rename = "type")]
    pub type_: ReasoningTextType,
    pub text: String,
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
pub enum ReasoningTextType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
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
pub enum ReasoningStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SummaryText {
    #[serde(rename = "type")]
    pub type_: SummaryTextType,
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ItemReference {
    #[serde(
        rename = "type",
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<Option<ItemReferenceType>>,
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
pub struct Compaction {
    #[serde(rename = "type")]
    pub type_: CompactionType,
    pub encrypted_content: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PendingSafetyCheck {
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub code: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub message: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum Caller {
    #[serde(rename = "direct")]
    Direct(DirectCaller),
    #[serde(rename = "program")]
    Program(ProgramCaller),
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
pub struct DirectCaller {
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
pub struct ProgramCaller {
    pub caller_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerAction {
    #[serde(rename = "click")]
    Click(ClickAction),
    #[serde(rename = "double_click")]
    DoubleClick(DoubleClickAction),
    #[serde(rename = "drag")]
    Drag(DragAction),
    #[serde(rename = "keypress")]
    Keypress(KeypressAction),
    #[serde(rename = "move")]
    Move(MoveAction),
    #[serde(rename = "screenshot")]
    Screenshot(ScreenshotAction),
    #[serde(rename = "scroll")]
    Scroll(ScrollAction),
    #[serde(rename = "type")]
    Type(TypeAction),
    #[serde(rename = "wait")]
    Wait(WaitAction),
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
pub enum ComputerScreenshotType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}
macro_rules! action_struct { ($name:ident { $($field:tt)* }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all = "snake_case")] #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)] #[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest } }; }
action_struct!(ClickAction { pub button: ClickButton, pub x: i64, pub y: i64, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub keys: Option<Option<Vec<String>>>, });
action_struct!(DoubleClickAction { pub x: i64, pub y: i64, #[wire(required)] #[serde(deserialize_with = "required_nullable")] pub keys: Option<Vec<String>>, });
action_struct!(DragAction { pub path: Vec<Coordinate>, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub keys: Option<Option<Vec<String>>>, });
action_struct!(KeypressAction { pub keys: Vec<String>, });
action_struct!(MoveAction { pub x: i64, pub y: i64, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub keys: Option<Option<Vec<String>>>, });
action_struct!(ScreenshotAction {});
action_struct!(ScrollAction { pub scroll_x: i64, pub scroll_y: i64, pub x: i64, pub y: i64, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub keys: Option<Option<Vec<String>>>, });
action_struct!(TypeAction { pub text: String, });
action_struct!(WaitAction {});
action_struct!(Coordinate { pub x: i64, pub y: i64, });

pub type ComputerToolCallOutput = ComputerScreenshot;
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerScreenshot {
    #[serde(rename = "type")]
    pub type_: ComputerScreenshotType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub image_url: Option<String>,
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
pub enum ClickButton {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "wheel")]
    Wheel,
    #[serde(rename = "back")]
    Back,
    #[serde(rename = "forward")]
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchAction {
    #[serde(rename = "search")]
    Search(WebSearchQuery),
    #[serde(rename = "open_page")]
    OpenPage(WebSearchOpenPage),
    #[serde(rename = "find_in_page")]
    FindInPage(WebSearchFindInPage),
}
action_struct!(WebSearchQuery { #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub queries: Option<Vec<String>>, #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub query: Option<String>, #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub sources: Option<Vec<WebSearchSource>>, });
action_struct!(WebSearchSource { #[serde(rename = "type")] pub type_: WebSearchSourceType, pub url: String, });
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
pub enum WebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}
action_struct!(WebSearchOpenPage { #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub url: Option<Option<String>>, });
action_struct!(WebSearchFindInPage { pub pattern: String, pub url: String, });
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeInterpreterOutput {
    #[serde(rename = "logs")]
    Logs(CodeLogs),
    #[serde(rename = "image")]
    Image(CodeImage),
}
action_struct!(CodeLogs { pub logs: String, });
action_struct!(CodeImage { pub url: String, });

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerCall {
    #[serde(rename = "type")]
    pub type_: ComputerCallType,
    pub id: String,
    pub call_id: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub action: Option<ComputerAction>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub actions: Option<Vec<ComputerAction>>,
    pub pending_safety_checks: Vec<PendingSafetyCheck>,
    pub status: ItemStatus,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ComputerCallOutput {
    #[serde(rename = "type")]
    pub type_: ComputerCallOutputType,
    pub call_id: String,
    pub output: ComputerToolCallOutput,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub acknowledged_safety_checks: Option<Option<Vec<PendingSafetyCheck>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<Option<ItemStatus>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchCall {
    #[serde(rename = "type")]
    pub type_: WebSearchCallType,
    pub id: String,
    /// The initial in-progress stream item precedes its search action.
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub action: Option<WebSearchAction>,
    pub status: WebSearchStatus,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FileSearchCall {
    #[serde(rename = "type")]
    pub type_: FileSearchCallType,
    pub id: String,
    pub queries: Vec<String>,
    pub status: FileSearchStatus,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub results: Option<Option<Vec<FileSearchResult>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FileSearchResult {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub attributes: Option<Option<std::collections::BTreeMap<String, FileAttributeValue>>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub file_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub filename: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub score: Option<serde_json::Number>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ImageGenerationCall {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub action: Option<Option<ImageAction>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub background: Option<Option<ImageBackground>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_format: Option<Option<ImageOutputFormat>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub quality: Option<Option<ImageQuality>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub revised_prompt: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub size: Option<Option<String>>,

    #[serde(rename = "type")]
    pub type_: ImageGenerationCallType,
    pub id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub result: Option<String>,
    pub status: ImageGenerationStatus,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeInterpreterCall {
    #[serde(rename = "type")]
    pub type_: CodeInterpreterCallType,
    pub id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub code: Option<String>,
    pub container_id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub outputs: Option<Vec<CodeInterpreterOutput>>,
    pub status: CodeInterpreterStatus,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CustomToolCall {
    #[serde(rename = "async", skip_serializing_if = "Option::is_none")]
    pub async_: Option<bool>,

    #[serde(rename = "type")]
    pub type_: CustomToolCallType,
    pub call_id: String,
    pub input: String,
    pub name: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub namespace: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CustomToolCallOutput {
    #[serde(rename = "type")]
    pub type_: CustomToolCallOutputType,
    pub call_id: String,
    pub output: CustomOutput,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<Caller>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputMessage {
    pub id: String,
    pub content: Vec<OutputContent>,
    pub role: OutputMessageRole,
    pub status: OutputMessageStatus,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub phase: Option<Option<MessagePhase>>,
    #[serde(rename = "type")]
    pub type_: MessageType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum OutputContent {
    Text(ResponseOutputText),
    Refusal(ResponseOutputRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum OutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum OutputMessageStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

macro_rules! simple_item {
    ($name:ident { $($field:tt)* }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
        #[serde(rename_all = "snake_case")]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        #[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest }
    };
}

simple_item!(ToolSearchCall {
    #[serde(rename = "type")] pub type_: ToolSearchCallType,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    // Get input token counts.md:1183 explicitly declares arbitrary unknown JSON.
    pub arguments: serde_json::Value,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub call_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub execution: Option<ToolExecution>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub status: Option<Option<ItemStatus>>,
});
simple_item!(ToolSearchOutput {
    #[serde(rename = "type")] pub type_: ToolSearchOutputType,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    pub tools: Vec<Tool>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub call_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub execution: Option<ToolExecution>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub status: Option<Option<ItemStatus>>,
});
simple_item!(AdditionalTools {
    #[serde(rename = "type")] pub type_: AdditionalToolsType,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    pub role: AdditionalToolsRole, pub tools: Vec<Tool>,
});
simple_item!(LocalShellCall {
    #[serde(rename = "type")] pub type_: LocalShellCallType,
    pub id: String, pub call_id: String, pub action: LocalShellAction,
    pub status: ItemStatus,
});
simple_item!(LocalShellCallOutput {
    #[serde(rename = "type")] pub type_: LocalShellCallOutputType,
    pub id: String, pub output: String, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub status: Option<Option<ItemStatus>>,
});
simple_item!(ShellCall {
    #[serde(rename = "type")] pub type_: ShellCallType,
    pub call_id: String, pub action: ShellAction,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub caller: Option<Option<Caller>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub environment: Option<Option<ShellCallEnvironment>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub status: Option<Option<ItemStatus>>,
});
simple_item!(ShellCallOutput {
    #[serde(rename = "type")] pub type_: ShellCallOutputType,
    pub call_id: String, pub output: Vec<ShellOutputContent>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub caller: Option<Option<Caller>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub max_output_length: Option<Option<i64>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub status: Option<Option<ItemStatus>>,
});
simple_item!(ApplyPatchCall {
    #[serde(rename = "type")] pub type_: ApplyPatchCallType,
    pub call_id: String, pub operation: ApplyPatchOperation, pub status: ApplyPatchStatus,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub caller: Option<Option<Caller>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
});
simple_item!(ApplyPatchCallOutput {
    #[serde(rename = "type")] pub type_: ApplyPatchCallOutputType,
    pub call_id: String, pub status: ApplyPatchOutputStatus, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub output: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub caller: Option<Option<Caller>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
});
simple_item!(McpListTools {
    #[serde(rename = "type")] pub type_: McpListToolsType,
    pub id: String, pub server_label: String, pub tools: Vec<McpToolDefinition>, #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub error: Option<Option<String>>,
});
simple_item!(McpApprovalRequest {
    #[serde(rename = "type")] pub type_: McpApprovalRequestType,
    pub id: String, pub arguments: String, pub name: String, pub server_label: String,
});
simple_item!(McpApprovalResponse {
    #[serde(rename = "type")] pub type_: McpApprovalResponseType,
    pub approval_request_id: String, pub approve: bool,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub reason: Option<Option<String>>,
});
simple_item!(McpCall {
    #[serde(rename = "type")] pub type_: McpCallType,
    pub id: String, pub arguments: String, pub name: String, pub server_label: String,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub approval_request_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub error: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")] pub output: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_optional", skip_serializing_if = "Option::is_none")] pub status: Option<McpCallStatus>,
});
simple_item!(CompactionTrigger { #[serde(rename = "type")] pub type_: CompactionTriggerType, });
simple_item!(Program {
    #[serde(rename = "type")] pub type_: ProgramType,
    pub id: String, pub call_id: String, pub code: String, pub fingerprint: String,
});
simple_item!(ProgramOutput {
    #[serde(rename = "type")] pub type_: ProgramOutputType,
    pub id: String, pub call_id: String, pub result: String, pub status: ProgramOutputStatus,
});

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolDefinition {
    pub name: String,
    // Get input token counts.md:3757: unknown schema, not a fixed wire object.
    pub input_schema: serde_json::Value,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    // Get input token counts.md:3765 permits arbitrary annotations JSON or null.
    pub annotations: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LocalShellAction {
    pub command: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(rename = "type")]
    pub type_: LocalShellActionType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_ms: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub user: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub working_directory: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ShellAction {
    pub commands: Vec<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_ms: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_length: Option<Option<i64>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ApplyPatchOperation {
    #[serde(rename = "create_file")]
    Create(ApplyPatchCreate),
    #[serde(rename = "delete_file")]
    Delete(ApplyPatchDelete),
    #[serde(rename = "update_file")]
    Update(ApplyPatchUpdate),
}
action_struct!(ApplyPatchCreate { pub path: String, pub diff: String, });
action_struct!(ApplyPatchDelete { pub path: String, });
action_struct!(ApplyPatchUpdate { pub path: String, pub diff: String, });
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ShellOutputContent {
    pub stdout: String,
    pub stderr: String,
    pub outcome: ShellOutputOutcome,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ShellOutputOutcome {
    #[serde(rename = "timeout")]
    Timeout(ShellTimeout),
    #[serde(rename = "exit")]
    Exit(ShellExit),
}
action_struct!(ShellTimeout {});
action_struct!(ShellExit { pub exit_code: i64, });

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextConfig {
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub format: Option<TextFormat>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub verbosity: Option<Option<TextVerbosity>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextFormat {
    #[serde(rename = "text")]
    Text(TextFormatText),
    #[serde(rename = "json_schema")]
    JsonSchema(TextFormatJsonSchema),
    #[serde(rename = "json_object")]
    JsonObject(TextFormatJsonObject),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextFormatText {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextFormatJsonSchema {
    pub name: String,
    // Get input token counts.md:4246: user-provided map[unknown] JSON Schema.
    pub schema: Rest,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub strict: Option<Option<bool>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextFormatJsonObject {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoice {
    Mode(ToolChoiceMode),
    Allowed(ToolChoiceAllowed),
    Hosted(ToolChoiceHosted),
    Function(ToolChoiceFunction),
    Mcp(ToolChoiceMcp),
    Custom(ToolChoiceCustom),
    Programmatic(ToolChoiceProgrammatic),
    ApplyPatch(ToolChoiceApplyPatch),
    Shell(ToolChoiceShell),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceHosted {
    #[serde(rename = "type")]
    pub type_: ToolChoiceHostedType,
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
pub enum ToolChoiceHostedType {
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "computer")]
    Computer,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
    #[serde(rename = "computer_use")]
    ComputerUse,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
    #[serde(rename = "image_generation")]
    ImageGeneration,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceAllowed {
    #[serde(rename = "type")]
    pub type_: ToolChoiceAllowedType,
    pub mode: AllowedToolChoiceMode,
    // Get input token counts.md:4334 declares array of map[unknown].
    pub tools: Vec<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceFunction {
    #[serde(rename = "type")]
    pub type_: ToolChoiceFunctionType,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceMcp {
    #[serde(rename = "type")]
    pub type_: ToolChoiceMcpType,
    pub server_label: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceCustom {
    #[serde(rename = "type")]
    pub type_: ToolChoiceCustomType,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseInputTextType {
    #[serde(rename = "input_text")]
    ResponseInputText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseInputImageType {
    #[serde(rename = "input_image")]
    ResponseInputImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseInputFileType {
    #[serde(rename = "input_file")]
    ResponseInputFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FunctionCallType {
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FunctionCallOutputType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningItemType {
    #[serde(rename = "reasoning")]
    ReasoningItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ItemReferenceType {
    #[serde(rename = "item_reference")]
    ItemReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CompactionType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerCallType {
    #[serde(rename = "computer_call")]
    ComputerCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ComputerCallOutputType {
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchCallType {
    #[serde(rename = "web_search_call")]
    WebSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FileSearchCallType {
    #[serde(rename = "file_search_call")]
    FileSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageGenerationCallType {
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeInterpreterCallType {
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CustomToolCallType {
    #[serde(rename = "custom_tool_call")]
    CustomToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CustomToolCallOutputType {
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum SummaryTextType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseOutputTextType {
    #[serde(rename = "output_text")]
    ResponseOutputText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseOutputRefusalType {
    #[serde(rename = "refusal")]
    ResponseOutputRefusal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceAllowedType {
    #[serde(rename = "allowed_tools")]
    ToolChoiceAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceFunctionType {
    #[serde(rename = "function")]
    ToolChoiceFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceMcpType {
    #[serde(rename = "mcp")]
    ToolChoiceMcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceCustomType {
    #[serde(rename = "custom")]
    ToolChoiceCustom,
}

macro_rules! item_tag {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
        #[derive(gproxy_protocol_macros::DeclaredFields)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}
item_tag!(ToolSearchCallType, ToolSearchCall, "tool_search_call");
item_tag!(ToolSearchOutputType, ToolSearchOutput, "tool_search_output");
item_tag!(AdditionalToolsType, AdditionalTools, "additional_tools");
item_tag!(LocalShellCallType, LocalShellCall, "local_shell_call");
item_tag!(
    LocalShellCallOutputType,
    LocalShellCallOutput,
    "local_shell_call_output"
);
item_tag!(ShellCallType, ShellCall, "shell_call");
item_tag!(ShellCallOutputType, ShellCallOutput, "shell_call_output");
item_tag!(ApplyPatchCallType, ApplyPatchCall, "apply_patch_call");
item_tag!(
    ApplyPatchCallOutputType,
    ApplyPatchCallOutput,
    "apply_patch_call_output"
);
item_tag!(McpListToolsType, McpListTools, "mcp_list_tools");
item_tag!(
    McpApprovalRequestType,
    McpApprovalRequest,
    "mcp_approval_request"
);
item_tag!(
    McpApprovalResponseType,
    McpApprovalResponse,
    "mcp_approval_response"
);
item_tag!(McpCallType, McpCall, "mcp_call");
item_tag!(
    CompactionTriggerType,
    CompactionTrigger,
    "compaction_trigger"
);
item_tag!(ProgramType, Program, "program");
item_tag!(ProgramOutputType, ProgramOutput, "program_output");
item_tag!(LocalShellActionType, Exec, "exec");
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
pub enum ProgramOutputStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
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
pub enum ItemStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MessageType {
    #[serde(rename = "message")]
    Message,
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
pub enum MessagePhase {
    #[serde(rename = "commentary")]
    Commentary,
    #[serde(rename = "final_answer")]
    FinalAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum InputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
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
pub enum AdditionalToolsRole {
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceMode {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AllowedToolChoiceMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceProgrammatic {
    #[serde(rename = "type")]
    pub type_: ToolChoiceProgrammaticType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceProgrammaticType {
    #[serde(rename = "programmatic_tool_calling")]
    Tag,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceApplyPatch {
    #[serde(rename = "type")]
    pub type_: ToolChoiceApplyPatchType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceApplyPatchType {
    #[serde(rename = "apply_patch")]
    Tag,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolChoiceShell {
    #[serde(rename = "type")]
    pub type_: ToolChoiceShellType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChoiceShellType {
    #[serde(rename = "shell")]
    Tag,
}

// Get input token counts.md:1019-1037 differs from ordinary input text:
// the function-output cache breakpoint explicitly permits null.
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
pub struct FunctionOutputText {
    #[serde(rename = "type")]
    pub type_: ResponseInputTextType,
    pub text: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_breakpoint: Option<Option<PromptCacheBreakpoint>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
// D:3896-3917 uses ordinary input content rather than the function-output variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CustomOutput {
    Text(String),
    Content(Vec<InputContent>),
}
// D:3469-3475: input shell calls accept local/reference, never container_auto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ShellCallEnvironment {
    Local(ShellLocalEnvironment),
    Reference(ShellContainerReference),
}
// D:466-478: file attributes are scalar values, not arbitrary JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FileAttributeValue {
    Text(String),
    Number(serde_json::Number),
    Boolean(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageDetail {
    Low,
    High,
    Auto,
    Original,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FileDetail {
    Auto,
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum PromptCacheMode {
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchStatus {
    InProgress,
    Searching,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FileSearchStatus {
    InProgress,
    Searching,
    Completed,
    Incomplete,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageGenerationStatus {
    InProgress,
    Completed,
    Generating,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeInterpreterStatus {
    InProgress,
    Completed,
    Incomplete,
    Interpreting,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ApplyPatchStatus {
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ApplyPatchOutputStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpCallStatus {
    InProgress,
    Completed,
    Incomplete,
    Calling,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    /// Codex custom effort is serialized as an unsigned JSON integer.
    #[serde(untagged)]
    Numeric(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningContext {
    Auto,
    CurrentTurn,
    AllTurns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextVerbosity {
    Low,
    Medium,
    High,
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
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConfigurationUpdateType {
    ConfigurationUpdate,
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
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConfigurationUpdate {
    #[serde(rename = "type")]
    pub type_: ConfigurationUpdateType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ConfigurationReasoning>,
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
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConfigurationReasoning {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub effort: Option<Option<ReasoningEffort>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
