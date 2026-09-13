//! OpenAI Responses input-token counting (`POST /responses/input_tokens`).
//!
//! The request uses the same native input and tool shapes as Responses.  The
//! types here deliberately remain OpenAI-shaped; they are not a shared IR.
//!
//! Initial schema coverage: several hosted-tool payloads remain raw JSON and
//! some input-item/tool variants are not yet modeled. HTTP envelopes are complete;
//! these body types are not yet the full upstream schema.
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CountTokensRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Input>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for InputContent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value.get("type").and_then(serde_json::Value::as_str);
        match kind {
            Some("input_text") => serde_json::from_value(value)
                .map(InputContent::Text)
                .map_err(serde::de::Error::custom),
            Some("input_image") => serde_json::from_value(value)
                .map(InputContent::Image)
                .map_err(serde::de::Error::custom),
            Some("input_file") => serde_json::from_value(value)
                .map(InputContent::File)
                .map_err(serde::de::Error::custom),
            Some(other) => Err(serde::de::Error::custom(format!(
                "unknown input content type {other}"
            ))),
            None => Err(serde::de::Error::custom("input content requires type")),
        }
    }
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
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputMessage {
    pub content: MessageContent,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputItem {
    Easy(EasyInputMessage),
    Message(InputMessage),
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
}

impl<'de> Deserialize<'de> for InputItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value.get("type").and_then(serde_json::Value::as_str);
        macro_rules! parse {
            ($variant:ident, $ty:ty) => {
                serde_json::from_value::<$ty>(value)
                    .map(InputItem::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }
        match kind {
            None => parse!(Easy, EasyInputMessage),
            Some("message") => parse!(Message, InputMessage),
            Some("function_call") => parse!(FunctionCall, FunctionCall),
            Some("function_call_output") => parse!(FunctionCallOutput, FunctionCallOutput),
            Some("reasoning") => parse!(Reasoning, ReasoningItem),
            Some("item_reference") => parse!(ItemReference, ItemReference),
            Some("compaction") => parse!(Compaction, Compaction),
            Some("computer_call") => parse!(ComputerCall, ComputerCall),
            Some("computer_call_output") => parse!(ComputerCallOutput, ComputerCallOutput),
            Some("web_search_call") => parse!(WebSearchCall, WebSearchCall),
            Some("file_search_call") => parse!(FileSearchCall, FileSearchCall),
            Some("image_generation_call") => parse!(ImageGenerationCall, ImageGenerationCall),
            Some("code_interpreter_call") => parse!(CodeInterpreterCall, CodeInterpreterCall),
            Some("custom_tool_call") => parse!(CustomToolCall, CustomToolCall),
            Some("custom_tool_call_output") => parse!(CustomToolCallOutput, CustomToolCallOutput),
            Some(other) => Err(serde::de::Error::custom(format!(
                "unknown input item type {other}"
            ))),
        }
    }
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
pub struct Reasoning {
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
    #[serde(rename = "type")]
    pub type_: ItemReferenceType,
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
pub struct ComputerCall {
    #[serde(rename = "type")]
    pub type_: ComputerCallType,
    pub id: String,
    pub call_id: String,
    pub action: serde_json::Value,
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
    pub output: serde_json::Value,
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
    pub action: serde_json::Value,
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<serde_json::Value>>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoice {
    Mode(String),
    Allowed(ToolChoiceAllowed),
    Hosted(ToolChoiceHosted),
    Function(ToolChoiceFunction),
    Mcp(ToolChoiceMcp),
    Custom(ToolChoiceCustom),
    Programmatic(ToolChoiceType),
    ApplyPatch(ToolChoiceType),
    Shell(ToolChoiceType),
}
impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(mode) = value.as_str() {
            return Ok(Self::Mode(mode.to_owned()));
        }
        macro_rules! parse {
            ($variant:ident) => {
                serde_json::from_value(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }
        match value.get("type").and_then(serde_json::Value::as_str) {
            Some("allowed_tools") => parse!(Allowed),
            Some("function") => parse!(Function),
            Some("mcp") => parse!(Mcp),
            Some("custom") => parse!(Custom),
            Some("programmatic_tool_calling") => parse!(Programmatic),
            Some("apply_patch") => parse!(ApplyPatch),
            Some("shell") => parse!(Shell),
            Some(_) => parse!(Hosted),
            None => Err(serde::de::Error::custom("tool choice requires a type")),
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolChoiceAllowed {
    #[serde(rename = "type")]
    pub type_: ToolChoiceAllowedType,
    pub mode: String,
    pub tools: Vec<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
pub struct ToolChoiceHosted {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
pub struct ToolChoiceType {
    #[serde(rename = "type")]
    pub type_: String,
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
    #[serde(rename = "web_search_preview")]
    WebSearch(WebSearchTool),
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
    pub parameters: Rest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    pub filters: Option<Rest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<Rest>,
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
    pub filters: Option<Rest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<Rest>,
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
    pub tools: Vec<Rest>,
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
    pub environment: Rest,
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
    pub execution: Option<Rest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Rest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CountTokensResponseBody {
    pub input_tokens: i64,
    pub object: CountTokensObject,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CountTokensObject {
    #[serde(rename = "response.input_tokens")]
    ResponseInputTokens,
}

pub type CountTokensRequest = crate::WireRequest<CountTokensRequestBody>;
pub type CountTokensResponse = crate::WireResponse<CountTokensResponseBody>;

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
