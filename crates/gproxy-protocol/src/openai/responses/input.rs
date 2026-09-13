//! Native OpenAI Responses input, output items, tool and configuration shapes.
//!
//! The request uses the same native input and tool shapes as Responses.  The
//! types here deliberately remain OpenAI-shaped; they are not a shared IR.
//!
//! Initial schema coverage: several hosted-tool payloads remain raw JSON and
//! some input-item/tool variants are not yet modeled. HTTP envelopes are complete;
//! these body types are not yet the full upstream schema.
use crate::Rest;
use serde::{Deserialize, Serialize};

/// A field which is required on the wire but explicitly permits JSON null.
/// `deserialize_with` preserves the distinction between a missing field
/// (serde reports an error) and a present null (returns `None`).
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn present_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
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
pub struct ResponseConversationParam {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationParam {
    Id(String),
    Object(ResponseConversationParam),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Input {
    Text(String),
    Items(Vec<InputItem>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<InputContent>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct ResponseInputText {
    #[serde(rename = "type")]
    pub type_: ResponseInputTextType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseInputImage {
    #[serde(rename = "type")]
    pub type_: ResponseInputImageType,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseInputFile {
    #[serde(rename = "type")]
    pub type_: ResponseInputFileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_breakpoint: Option<PromptCacheBreakpoint>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PromptCacheBreakpoint {
    pub mode: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EasyInputMessage {
    pub content: MessageContent,
    pub role: MessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<MessageType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputMessage {
    pub content: Vec<InputContent>,
    pub role: InputMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OutputMessageStatus>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<MessageType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputItem {
    OutputMessage(ResponseOutputMessage),
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseOutputText {
    #[serde(rename = "type")]
    pub type_: ResponseOutputTextType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<OutputAnnotation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<OutputLogprob>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputAnnotation {
    File(FileCitation),
    Url(UrlCitation),
    ContainerFile(ContainerFileCitation),
    Path(FilePath),
}
macro_rules! citation { ($name:ident { $($field:tt)* }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all = "snake_case")] pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest } }; }
citation!(FileCitation { pub file_id: String, pub filename: String, pub index: i64, #[serde(rename = "type")] pub type_: String, });
citation!(UrlCitation { pub end_index: i64, pub start_index: i64, pub title: String, pub url: String, #[serde(rename = "type")] pub type_: String, });
citation!(ContainerFileCitation { pub container_id: String, pub end_index: i64, pub file_id: String, pub filename: String, pub start_index: i64, #[serde(rename = "type")] pub type_: String, });
citation!(FilePath { pub file_id: String, pub index: i64, #[serde(rename = "type")] pub type_: String, });
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
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
pub struct FunctionCall {
    #[serde(rename = "type")]
    pub type_: FunctionCallType,
    pub arguments: String,
    pub call_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionCallOutput {
    #[serde(rename = "type")]
    pub type_: FunctionCallOutputType,
    pub call_id: String,
    pub output: FunctionOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FunctionOutput {
    Text(String),
    Content(Vec<InputContent>),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ReasoningItem {
    #[serde(rename = "type")]
    pub type_: ReasoningItemType,
    pub id: String,
    pub summary: Vec<SummaryText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct Compaction {
    #[serde(rename = "type")]
    pub type_: CompactionType,
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PendingSafetyCheck {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Caller {
    #[serde(rename = "direct")]
    Direct(DirectCaller),
    #[serde(rename = "program")]
    Program(ProgramCaller),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
pub struct DirectCaller {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
pub struct ProgramCaller {
    pub caller_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
macro_rules! action_struct { ($name:ident { $($field:tt)* }) => { #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all = "snake_case")] #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)] pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest } }; }
action_struct!(ClickAction { pub button: String, pub x: i64, pub y: i64, #[serde(skip_serializing_if = "Option::is_none")] pub keys: Option<Vec<String>>, });
action_struct!(DoubleClickAction { pub x: i64, pub y: i64, #[serde(skip_serializing_if = "Option::is_none")] pub keys: Option<Vec<String>>, });
action_struct!(DragAction { pub path: Vec<Coordinate>, #[serde(skip_serializing_if = "Option::is_none")] pub keys: Option<Vec<String>>, });
action_struct!(KeypressAction { pub keys: Vec<String>, });
action_struct!(MoveAction { pub x: i64, pub y: i64, #[serde(skip_serializing_if = "Option::is_none")] pub keys: Option<Vec<String>>, });
action_struct!(ScreenshotAction {});
action_struct!(ScrollAction { pub scroll_x: i64, pub scroll_y: i64, #[serde(skip_serializing_if = "Option::is_none")] pub keys: Option<Vec<String>>, });
action_struct!(TypeAction { pub text: String, });
action_struct!(WaitAction {});
action_struct!(Coordinate { pub x: i64, pub y: i64, });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComputerToolCallOutput {
    Screenshot(ComputerScreenshot),
    Other(ComputerOutputImage),
}
action_struct!(ComputerScreenshot { pub image_url: String, });
action_struct!(ComputerOutputImage { #[serde(rename = "type")] pub type_: String, pub data: Option<String>, });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSearchAction {
    #[serde(rename = "search")]
    Search(WebSearchQuery),
    #[serde(rename = "open_page")]
    OpenPage(WebSearchOpenPage),
    #[serde(rename = "find_in_page")]
    FindInPage(WebSearchFindInPage),
}
action_struct!(WebSearchQuery { pub queries: Vec<String>, #[serde(skip_serializing_if = "Option::is_none")] pub query: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] pub sources: Option<Vec<String>>, });
action_struct!(WebSearchOpenPage { pub url: String, });
action_struct!(WebSearchFindInPage { pub pattern: String, pub url: String, });
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
pub struct ComputerCall {
    #[serde(rename = "type")]
    pub type_: ComputerCallType,
    pub id: String,
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ComputerAction>,
    pub pending_safety_checks: Vec<PendingSafetyCheck>,
    pub status: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerCallOutput {
    #[serde(rename = "type")]
    pub type_: ComputerCallOutputType,
    pub call_id: String,
    pub output: ComputerToolCallOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<PendingSafetyCheck>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchCall {
    #[serde(rename = "type")]
    pub type_: WebSearchCallType,
    pub id: String,
    pub action: WebSearchAction,
    pub status: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchCall {
    #[serde(rename = "type")]
    pub type_: FileSearchCallType,
    pub id: String,
    pub queries: Vec<String>,
    pub status: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageGenerationCall {
    #[serde(rename = "type")]
    pub type_: ImageGenerationCallType,
    pub id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub result: Option<String>,
    pub status: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
    pub status: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolCall {
    #[serde(rename = "type")]
    pub type_: CustomToolCallType,
    pub call_id: String,
    pub input: String,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomToolCallOutput {
    #[serde(rename = "type")]
    pub type_: CustomToolCallOutputType,
    pub call_id: String,
    pub output: FunctionOutput,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseOutputMessage {
    pub id: String,
    pub content: Vec<OutputContent>,
    pub role: OutputMessageRole,
    pub status: OutputMessageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(rename = "type")]
    pub type_: MessageType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputContent {
    Text(ResponseOutputText),
    Refusal(ResponseOutputRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
        pub struct $name { $($field)* #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")] pub rest: Rest }
    };
}

simple_item!(ToolSearchCall {
    #[serde(rename = "type")] pub type_: ToolSearchCallType,
    pub id: String, pub arguments: String,
});
simple_item!(ToolSearchOutput {
    #[serde(rename = "type")] pub type_: ToolSearchOutputType,
    pub id: String, pub tools: Vec<Tool>,
});
simple_item!(AdditionalTools {
    #[serde(rename = "type")] pub type_: AdditionalToolsType,
    pub id: String, pub role: String, pub tools: Vec<Tool>,
});
simple_item!(LocalShellCall {
    #[serde(rename = "type")] pub type_: LocalShellCallType,
    pub id: String, pub call_id: String, pub action: LocalShellAction,
    #[serde(skip_serializing_if = "Option::is_none")] pub status: Option<String>,
});
simple_item!(LocalShellCallOutput {
    #[serde(rename = "type")] pub type_: LocalShellCallOutputType,
    pub id: String, pub output: String, pub status: String,
});
simple_item!(ShellCall {
    #[serde(rename = "type")] pub type_: ShellCallType,
    pub call_id: String, pub action: ShellAction,
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub status: Option<String>,
});
simple_item!(ShellCallOutput {
    #[serde(rename = "type")] pub type_: ShellCallOutputType,
    pub call_id: String, pub output: Vec<ShellOutputContent>,
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub status: Option<String>,
});
simple_item!(ApplyPatchCall {
    #[serde(rename = "type")] pub type_: ApplyPatchCallType,
    pub call_id: String, pub operation: ApplyPatchOperation,
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub status: Option<String>,
});
simple_item!(ApplyPatchCallOutput {
    #[serde(rename = "type")] pub type_: ApplyPatchCallOutputType,
    pub call_id: String, pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub id: Option<String>,
});
simple_item!(McpListTools {
    #[serde(rename = "type")] pub type_: McpListToolsType,
    pub id: String, pub server_label: String, pub tools: Vec<McpToolDefinition>,
});
simple_item!(McpApprovalRequest {
    #[serde(rename = "type")] pub type_: McpApprovalRequestType,
    pub id: String, pub arguments: String, pub name: String, pub server_label: String,
});
simple_item!(McpApprovalResponse {
    #[serde(rename = "type")] pub type_: McpApprovalResponseType,
    pub approval_request_id: String, pub approve: bool,
});
simple_item!(McpCall {
    #[serde(rename = "type")] pub type_: McpCallType,
    pub id: String, pub arguments: String, pub name: String, pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub status: Option<String>,
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
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LocalShellAction {
    pub command: Vec<String>,
    pub env: Rest,
    #[serde(rename = "type")]
    pub type_: LocalShellActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ShellAction {
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
pub struct ShellOutputContent {
    pub stdout: String,
    pub stderr: String,
    pub outcome: ShellOutputOutcome,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
pub struct TextConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct TextFormatText {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextFormatJsonSchema {
    pub name: String,
    pub schema: Rest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TextFormatJsonObject {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct ToolChoiceHosted {
    #[serde(rename = "type")]
    pub type_: ToolChoiceHostedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct ToolChoiceAllowed {
    #[serde(rename = "type")]
    pub type_: ToolChoiceAllowedType,
    pub mode: AllowedToolChoiceMode,
    pub tools: Vec<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
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
pub struct ToolChoiceMcp {
    #[serde(rename = "type")]
    pub type_: ToolChoiceMcpType,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolChoiceCustom {
    #[serde(rename = "type")]
    pub type_: ToolChoiceCustomType,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Tool {
    #[serde(rename = "function")]
    Function(FunctionTool),
    #[serde(rename = "file_search")]
    FileSearch(FileSearchTool),
    #[serde(rename = "computer_use_preview")]
    Computer(ComputerTool),
    #[serde(rename = "computer")]
    ComputerHosted(ToolMarker),
    #[serde(rename = "web_search_preview")]
    WebSearch(WebSearchTool),
    #[serde(rename = "web_search")]
    WebSearchLegacy(WebSearchTool),
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch2025(WebSearchTool),
    #[serde(rename = "code_interpreter")]
    CodeInterpreter(CodeInterpreterTool),
    #[serde(rename = "custom")]
    Custom(CustomTool),
    #[serde(rename = "namespace")]
    Namespace(NamespaceTool),
    #[serde(rename = "local_shell")]
    LocalShell(ToolMarker),
    #[serde(rename = "shell")]
    Shell(ShellTool),
    #[serde(rename = "image_generation")]
    ImageGeneration(ImageGenerationTool),
    #[serde(rename = "mcp")]
    Mcp(McpTool),
    #[serde(rename = "programmatic_tool_calling")]
    Programmatic(ToolMarker),
    #[serde(rename = "apply_patch")]
    ApplyPatch(ToolMarker),
    #[serde(rename = "tool_search")]
    ToolSearch(ToolSearchTool),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FunctionTool {
    pub name: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub parameters: Option<Rest>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FileSearchTool {
    pub vector_store_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<FileSearchFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<RankingOptions>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ComputerTool {
    pub display_height: i64,
    pub display_width: i64,
    pub environment: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<UserLocation>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CodeInterpreterTool {
    pub container: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CustomTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Rest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct NamespaceTool {
    pub description: String,
    pub name: String,
    pub tools: Vec<NamespaceToolDefinition>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolMarker {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ShellTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<ShellEnvironment>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageGenerationTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpTool {
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolExecution {
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "client")]
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileSearchFilter {
    Comparison(ComparisonFilter),
    Compound(CompoundFilter),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct ComparisonFilter {
    pub key: String,
    pub value: FilterValue,
    pub op: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    String(String),
    Number(serde_json::Number),
    Boolean(bool),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct CompoundFilter {
    pub filters: Vec<FileSearchFilter>,
    pub r#type: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct RankingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<HybridSearch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<serde_json::Number>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct HybridSearch {
    pub embedding_weight: serde_json::Number,
    pub text_weight: serde_json::Number,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct UserLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct NamespaceToolDefinition {
    pub name: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShellEnvironment {
    Auto(ShellContainerAuto),
    Local(ShellLocalEnvironment),
    Reference(ShellContainerReference),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct ShellContainerAuto {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct ShellLocalEnvironment {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
pub struct ShellContainerReference {
    pub r#type: String,
    pub container_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseInputTextType {
    #[serde(rename = "input_text")]
    ResponseInputText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseInputImageType {
    #[serde(rename = "input_image")]
    ResponseInputImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseInputFileType {
    #[serde(rename = "input_file")]
    ResponseInputFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FunctionCallType {
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FunctionCallOutputType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ReasoningItemType {
    #[serde(rename = "reasoning")]
    ReasoningItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ItemReferenceType {
    #[serde(rename = "item_reference")]
    ItemReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CompactionType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerCallType {
    #[serde(rename = "computer_call")]
    ComputerCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ComputerCallOutputType {
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum WebSearchCallType {
    #[serde(rename = "web_search_call")]
    WebSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FileSearchCallType {
    #[serde(rename = "file_search_call")]
    FileSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageGenerationCallType {
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CodeInterpreterCallType {
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CustomToolCallType {
    #[serde(rename = "custom_tool_call")]
    CustomToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CustomToolCallOutputType {
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SummaryTextType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseOutputTextType {
    #[serde(rename = "output_text")]
    ResponseOutputText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseOutputRefusalType {
    #[serde(rename = "refusal")]
    ResponseOutputRefusal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceAllowedType {
    #[serde(rename = "allowed_tools")]
    ToolChoiceAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceFunctionType {
    #[serde(rename = "function")]
    ToolChoiceFunction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceMcpType {
    #[serde(rename = "mcp")]
    ToolChoiceMcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceCustomType {
    #[serde(rename = "custom")]
    ToolChoiceCustom,
}

macro_rules! item_tag {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgramOutputStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum MessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub enum InputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct ToolChoiceProgrammatic {
    #[serde(rename = "type")]
    pub type_: ToolChoiceProgrammaticType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceProgrammaticType {
    #[serde(rename = "programmatic_tool_calling")]
    Tag,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolChoiceApplyPatch {
    #[serde(rename = "type")]
    pub type_: ToolChoiceApplyPatchType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceApplyPatchType {
    #[serde(rename = "apply_patch")]
    Tag,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolChoiceShell {
    #[serde(rename = "type")]
    pub type_: ToolChoiceShellType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoiceShellType {
    #[serde(rename = "shell")]
    Tag,
}
