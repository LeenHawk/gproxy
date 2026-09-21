//! HTTP Responses SSE JSON events, Responses.md:125798-136308.
//! AnnotationAdded.annotation is explicitly `unknown` (136212), hence Value.
use super::input::present_optional;
use super::response::{GenerateContentResponseBody, ResponseOutputItem};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Typed payloads for the HTTP Responses SSE stream. SSE framing and `[DONE]`
/// handling remain transport concerns; this enum only represents JSON data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum StreamEvent {
    #[serde(rename = "response.created")]
    Created(ResponseCreated),
    #[serde(rename = "response.queued")]
    Queued(ResponseQueued),
    #[serde(rename = "response.in_progress")]
    InProgress(ResponseInProgress),
    #[serde(rename = "response.completed")]
    Completed(ResponseCompleted),
    #[serde(rename = "response.failed")]
    Failed(ResponseFailed),
    #[serde(rename = "response.incomplete")]
    Incomplete(ResponseIncomplete),
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded(OutputItemEvent),
    #[serde(rename = "response.output_item.done")]
    OutputItemDone(OutputItemEvent),
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded(ContentPartEvent),
    #[serde(rename = "response.content_part.done")]
    ContentPartDone(ContentPartEvent),
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta(OutputTextDelta),
    #[serde(rename = "response.output_text.done")]
    OutputTextDone(OutputTextDone),
    #[serde(rename = "response.output_text.annotation.added")]
    OutputTextAnnotationAdded(OutputTextAnnotationAdded),
    #[serde(rename = "response.refusal.delta")]
    RefusalDelta(RefusalDelta),
    #[serde(rename = "response.refusal.done")]
    RefusalDone(RefusalDone),
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta(ReasoningTextDelta),
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone(ReasoningTextDone),
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded(ReasoningSummaryPartAddedEvent),
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone(ReasoningSummaryPartDoneEvent),
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta(ReasoningSummaryTextDelta),
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone(ReasoningSummaryTextDone),
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(FunctionCallArgumentsDelta),
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(FunctionCallArgumentsDone),
    #[serde(rename = "response.custom_tool_call_input.delta")]
    CustomToolInputDelta(CustomToolInputDelta),
    #[serde(rename = "response.custom_tool_call_input.done")]
    CustomToolInputDone(CustomToolInputDone),
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    CodeInterpreterCodeDelta(CodeInterpreterCodeDelta),
    #[serde(rename = "response.code_interpreter_call_code.done")]
    CodeInterpreterCodeDone(CodeInterpreterCodeDone),
    #[serde(rename = "response.audio.delta")]
    AudioDelta(AudioDelta),
    #[serde(rename = "response.audio.done")]
    AudioDone(AudioDone),
    #[serde(rename = "response.audio.transcript.delta")]
    AudioTranscriptDelta(AudioTranscriptDelta),
    #[serde(rename = "response.audio.transcript.done")]
    AudioTranscriptDone(AudioTranscriptDone),
    #[serde(rename = "response.image_generation_call.partial_image")]
    ImagePartial(ImagePartial),
    #[serde(rename = "response.image_generation_call.in_progress")]
    ImageCall(ImageCallEvent),
    #[serde(rename = "response.image_generation_call.generating")]
    ImageGenerating(ImageCallEvent),
    #[serde(rename = "response.image_generation_call.completed")]
    ImageCompleted(ImageCallEvent),
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    CodeInterpreterInProgress(CodeInterpreterEvent),
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    CodeInterpreterInterpreting(CodeInterpreterEvent),
    #[serde(rename = "response.code_interpreter_call.completed")]
    CodeInterpreterCompleted(CodeInterpreterEvent),
    #[serde(rename = "response.file_search_call.in_progress")]
    FileSearchInProgress(ToolCallEvent),
    #[serde(rename = "response.file_search_call.searching")]
    FileSearchSearching(ToolCallEvent),
    #[serde(rename = "response.file_search_call.completed")]
    FileSearchCompleted(ToolCallEvent),
    #[serde(rename = "response.web_search_call.in_progress")]
    WebSearchInProgress(ToolCallEvent),
    #[serde(rename = "response.web_search_call.searching")]
    WebSearchSearching(ToolCallEvent),
    #[serde(rename = "response.web_search_call.completed")]
    WebSearchCompleted(ToolCallEvent),
    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpArgumentsDelta(McpArgumentsDelta),
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpArgumentsDone(McpArgumentsDone),
    #[serde(rename = "response.mcp_call.in_progress")]
    McpInProgress(McpCallEvent),
    #[serde(rename = "response.mcp_call.completed")]
    McpCompleted(McpCallEvent),
    #[serde(rename = "response.mcp_call.failed")]
    McpFailed(McpCallEvent),
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    McpListToolsInProgress(McpListToolsEvent),
    #[serde(rename = "response.mcp_list_tools.completed")]
    McpListToolsCompleted(McpListToolsEvent),
    #[serde(rename = "response.mcp_list_tools.failed")]
    McpListToolsFailed(McpListToolsEvent),
    #[serde(rename = "error")]
    Error(ResponseErrorEvent),
}
macro_rules! event_struct { ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => { #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all="snake_case")] #[cfg_attr(not(feature="exhaustive"), non_exhaustive)] #[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct $name { pub sequence_number: i64, $(pub $field: $ty,)* #[serde(default, flatten, skip_serializing_if="serde_json::Map::is_empty")] pub rest: Rest } }; }
event_struct!(ResponseCreated {
    response: GenerateContentResponseBody
});
event_struct!(ResponseQueued {
    response: GenerateContentResponseBody
});
event_struct!(ResponseInProgress {
    response: GenerateContentResponseBody
});
event_struct!(ResponseCompleted {
    response: GenerateContentResponseBody
});
event_struct!(ResponseFailed {
    response: GenerateContentResponseBody
});
event_struct!(ResponseIncomplete {
    response: GenerateContentResponseBody
});
event_struct!(OutputItemEvent {
    output_index: i64,
    item: ResponseOutputItem
});
event_struct!(ContentPartEvent {
    item_id: String,
    output_index: i64,
    content_index: i64,
    part: OutputContentPart
});
event_struct!(OutputTextDelta { item_id: String, output_index: i64, content_index: i64, delta: String, logprobs: Vec<StreamLogprob> });
event_struct!(OutputTextDone { item_id: String, output_index: i64, content_index: i64, text: String, logprobs: Vec<StreamLogprob> });
event_struct!(OutputTextAnnotationAdded {
    item_id: String,
    output_index: i64,
    content_index: i64,
    annotation_index: i64,
    annotation: serde_json::Value
});
event_struct!(RefusalDelta {
    item_id: String,
    output_index: i64,
    content_index: i64,
    delta: String
});
event_struct!(RefusalDone {
    item_id: String,
    output_index: i64,
    content_index: i64,
    refusal: String
});
event_struct!(ReasoningTextDelta {
    item_id: String,
    output_index: i64,
    content_index: i64,
    delta: String
});
event_struct!(ReasoningTextDone {
    item_id: String,
    output_index: i64,
    content_index: i64,
    text: String
});
event_struct!(ReasoningSummaryPartAddedEvent {
    item_id: String,
    output_index: i64,
    summary_index: i64,
    part: ReasoningSummaryPart
});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningSummaryPartDoneEvent {
    pub sequence_number: i64,
    pub item_id: String,
    pub output_index: i64,
    pub summary_index: i64,
    pub part: ReasoningSummaryPart,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<SummaryPartStatus>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum SummaryPartStatus {
    #[serde(rename = "incomplete")]
    Incomplete,
}
event_struct!(ReasoningSummaryTextDelta {
    item_id: String,
    output_index: i64,
    summary_index: i64,
    delta: String
});
event_struct!(ReasoningSummaryTextDone {
    item_id: String,
    output_index: i64,
    summary_index: i64,
    text: String
});
event_struct!(FunctionCallArgumentsDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
event_struct!(FunctionCallArgumentsDone {
    item_id: String,
    output_index: i64,
    name: String,
    arguments: String
});
event_struct!(CustomToolInputDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
event_struct!(CustomToolInputDone {
    item_id: String,
    output_index: i64,
    input: String
});
event_struct!(CodeInterpreterCodeDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
event_struct!(CodeInterpreterCodeDone {
    item_id: String,
    output_index: i64,
    code: String
});
event_struct!(AudioDelta { delta: String });
event_struct!(AudioDone {});
event_struct!(AudioTranscriptDelta { delta: String });
event_struct!(AudioTranscriptDone {});
event_struct!(ImagePartial {
    item_id: String,
    output_index: i64,
    partial_image_index: i64,
    partial_image_b64: String
});
event_struct!(ImageCall {
    item_id: String,
    output_index: i64
});
pub type ImageCallEvent = ImageCall;
event_struct!(CodeInterpreterEvent {
    item_id: String,
    output_index: i64
});
event_struct!(ToolCallEvent {
    item_id: String,
    output_index: i64
});
event_struct!(McpArgumentsDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
event_struct!(McpArgumentsDone {
    item_id: String,
    output_index: i64,
    arguments: String
});
event_struct!(McpCallEvent {
    item_id: String,
    output_index: i64
});
event_struct!(McpListToolsEvent {
    item_id: String,
    output_index: i64
});
pub type ResponseStream = crate::WireResponse<crate::connection::ByteStream>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum OutputContentPart {
    Text(super::input::ResponseOutputText),
    Refusal(super::input::ResponseOutputRefusal),
    Reasoning(ReasoningText),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningText {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ReasoningTextType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningTextType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct StreamLogprob {
    pub token: String,
    pub logprob: serde_json::Number,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub top_logprobs: Option<Vec<StreamTopLogprob>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct StreamTopLogprob {
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub token: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub logprob: Option<serde_json::Number>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ReasoningSummaryPart {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ReasoningSummaryPartType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ReasoningSummaryPartType {
    #[serde(rename = "summary_text")]
    SummaryText,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseErrorEvent {
    #[wire(required)]
    #[serde(deserialize_with = "super::input::required_nullable")]
    pub code: Option<String>,
    pub message: String,
    #[wire(required)]
    #[serde(deserialize_with = "super::input::required_nullable")]
    pub param: Option<String>,
    pub sequence_number: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
