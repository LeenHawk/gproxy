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
    /// Undocumented upstream heartbeat; converted to Claude ping or otherwise ignored.
    #[serde(rename = "keepalive")]
    Keepalive,
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
macro_rules! agent_event_struct { ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => { #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)] #[serde(rename_all="snake_case")] #[cfg_attr(not(feature="exhaustive"), non_exhaustive)] #[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct $name { #[serde(default, deserialize_with = "super::input::present_nullable", skip_serializing_if = "Option::is_none")] pub agent: Option<Option<super::multi_agent::Agent>>, pub sequence_number: i64, $(pub $field: $ty,)* #[serde(default, flatten, skip_serializing_if="serde_json::Map::is_empty")] pub rest: Rest } }; }
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
agent_event_struct!(OutputItemEvent {
    output_index: i64,
    item: ResponseOutputItem
});
agent_event_struct!(ContentPartEvent {
    item_id: String,
    output_index: i64,
    content_index: i64,
    part: OutputContentPart
});
agent_event_struct!(OutputTextDelta { item_id: String, output_index: i64, content_index: i64, delta: String, logprobs: Vec<StreamLogprob> });
agent_event_struct!(OutputTextDone { item_id: String, output_index: i64, content_index: i64, text: String, logprobs: Vec<StreamLogprob> });
agent_event_struct!(OutputTextAnnotationAdded {
    item_id: String,
    output_index: i64,
    content_index: i64,
    annotation_index: i64,
    annotation: serde_json::Value
});
agent_event_struct!(RefusalDelta {
    item_id: String,
    output_index: i64,
    content_index: i64,
    delta: String
});
agent_event_struct!(RefusalDone {
    item_id: String,
    output_index: i64,
    content_index: i64,
    refusal: String
});
agent_event_struct!(ReasoningTextDelta {
    item_id: String,
    output_index: i64,
    content_index: i64,
    delta: String
});
agent_event_struct!(ReasoningTextDone {
    item_id: String,
    output_index: i64,
    content_index: i64,
    text: String
});
agent_event_struct!(ReasoningSummaryPartAddedEvent {
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
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
agent_event_struct!(ReasoningSummaryTextDelta {
    item_id: String,
    output_index: i64,
    summary_index: i64,
    delta: String
});
agent_event_struct!(ReasoningSummaryTextDone {
    item_id: String,
    output_index: i64,
    summary_index: i64,
    text: String
});
agent_event_struct!(FunctionCallArgumentsDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
agent_event_struct!(FunctionCallArgumentsDone {
    item_id: String,
    output_index: i64,
    name: String,
    arguments: String
});
agent_event_struct!(CustomToolInputDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
agent_event_struct!(CustomToolInputDone {
    item_id: String,
    output_index: i64,
    input: String
});
agent_event_struct!(CodeInterpreterCodeDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
agent_event_struct!(CodeInterpreterCodeDone {
    item_id: String,
    output_index: i64,
    code: String
});
agent_event_struct!(AudioDelta { delta: String });
agent_event_struct!(AudioDone {});
agent_event_struct!(AudioTranscriptDelta { delta: String });
agent_event_struct!(AudioTranscriptDone {});
agent_event_struct!(ImagePartial {
    item_id: String,
    output_index: i64,
    partial_image_index: i64,
    partial_image_b64: String
});
agent_event_struct!(ImageCall {
    item_id: String,
    output_index: i64
});
pub type ImageCallEvent = ImageCall;
agent_event_struct!(CodeInterpreterEvent {
    item_id: String,
    output_index: i64
});
agent_event_struct!(ToolCallEvent {
    item_id: String,
    output_index: i64
});
agent_event_struct!(McpArgumentsDelta {
    item_id: String,
    output_index: i64,
    delta: String
});
agent_event_struct!(McpArgumentsDone {
    item_id: String,
    output_index: i64,
    arguments: String
});
agent_event_struct!(McpCallEvent {
    item_id: String,
    output_index: i64
});
agent_event_struct!(McpListToolsEvent {
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

impl StreamEvent {
    pub fn agent(&self) -> Option<&super::multi_agent::Agent> {
        match self {
            Self::OutputItemAdded(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::OutputItemDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ContentPartAdded(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ContentPartDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::OutputTextDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::OutputTextDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::OutputTextAnnotationAdded(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::RefusalDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::RefusalDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningTextDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningTextDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningSummaryPartAdded(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningSummaryPartDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningSummaryTextDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ReasoningSummaryTextDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::FunctionCallArgumentsDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::FunctionCallArgumentsDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CustomToolInputDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CustomToolInputDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CodeInterpreterCodeDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CodeInterpreterCodeDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::AudioDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::AudioDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::AudioTranscriptDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::AudioTranscriptDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ImagePartial(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ImageCall(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ImageGenerating(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::ImageCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CodeInterpreterInProgress(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CodeInterpreterInterpreting(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::CodeInterpreterCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::FileSearchInProgress(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::FileSearchSearching(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::FileSearchCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::WebSearchInProgress(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::WebSearchSearching(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::WebSearchCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpArgumentsDelta(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpArgumentsDone(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpInProgress(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpFailed(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpListToolsInProgress(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpListToolsCompleted(v) => v.agent.as_ref().and_then(Option::as_ref),
            Self::McpListToolsFailed(v) => v.agent.as_ref().and_then(Option::as_ref),
            _ => None,
        }
    }
    pub(crate) fn set_agent(&mut self, agent: Option<super::multi_agent::Agent>) {
        match self {
            Self::OutputItemAdded(v) => v.agent = agent.map(Some),
            Self::OutputItemDone(v) => v.agent = agent.map(Some),
            Self::ContentPartAdded(v) => v.agent = agent.map(Some),
            Self::ContentPartDone(v) => v.agent = agent.map(Some),
            Self::OutputTextDelta(v) => v.agent = agent.map(Some),
            Self::OutputTextDone(v) => v.agent = agent.map(Some),
            Self::OutputTextAnnotationAdded(v) => v.agent = agent.map(Some),
            Self::RefusalDelta(v) => v.agent = agent.map(Some),
            Self::RefusalDone(v) => v.agent = agent.map(Some),
            Self::ReasoningTextDelta(v) => v.agent = agent.map(Some),
            Self::ReasoningTextDone(v) => v.agent = agent.map(Some),
            Self::ReasoningSummaryPartAdded(v) => v.agent = agent.map(Some),
            Self::ReasoningSummaryPartDone(v) => v.agent = agent.map(Some),
            Self::ReasoningSummaryTextDelta(v) => v.agent = agent.map(Some),
            Self::ReasoningSummaryTextDone(v) => v.agent = agent.map(Some),
            Self::FunctionCallArgumentsDelta(v) => v.agent = agent.map(Some),
            Self::FunctionCallArgumentsDone(v) => v.agent = agent.map(Some),
            Self::CustomToolInputDelta(v) => v.agent = agent.map(Some),
            Self::CustomToolInputDone(v) => v.agent = agent.map(Some),
            Self::CodeInterpreterCodeDelta(v) => v.agent = agent.map(Some),
            Self::CodeInterpreterCodeDone(v) => v.agent = agent.map(Some),
            Self::AudioDelta(v) => v.agent = agent.map(Some),
            Self::AudioDone(v) => v.agent = agent.map(Some),
            Self::AudioTranscriptDelta(v) => v.agent = agent.map(Some),
            Self::AudioTranscriptDone(v) => v.agent = agent.map(Some),
            Self::ImagePartial(v) => v.agent = agent.map(Some),
            Self::ImageCall(v) => v.agent = agent.map(Some),
            Self::ImageGenerating(v) => v.agent = agent.map(Some),
            Self::ImageCompleted(v) => v.agent = agent.map(Some),
            Self::CodeInterpreterInProgress(v) => v.agent = agent.map(Some),
            Self::CodeInterpreterInterpreting(v) => v.agent = agent.map(Some),
            Self::CodeInterpreterCompleted(v) => v.agent = agent.map(Some),
            Self::FileSearchInProgress(v) => v.agent = agent.map(Some),
            Self::FileSearchSearching(v) => v.agent = agent.map(Some),
            Self::FileSearchCompleted(v) => v.agent = agent.map(Some),
            Self::WebSearchInProgress(v) => v.agent = agent.map(Some),
            Self::WebSearchSearching(v) => v.agent = agent.map(Some),
            Self::WebSearchCompleted(v) => v.agent = agent.map(Some),
            Self::McpArgumentsDelta(v) => v.agent = agent.map(Some),
            Self::McpArgumentsDone(v) => v.agent = agent.map(Some),
            Self::McpInProgress(v) => v.agent = agent.map(Some),
            Self::McpCompleted(v) => v.agent = agent.map(Some),
            Self::McpFailed(v) => v.agent = agent.map(Some),
            Self::McpListToolsInProgress(v) => v.agent = agent.map(Some),
            Self::McpListToolsCompleted(v) => v.agent = agent.map(Some),
            Self::McpListToolsFailed(v) => v.agent = agent.map(Some),
            _ => {}
        }
    }
}

impl StreamEvent {
    pub(crate) fn output_index(&self) -> Option<i64> {
        match self {
            Self::OutputItemAdded(v) => Some(v.output_index),
            Self::OutputItemDone(v) => Some(v.output_index),
            Self::ContentPartAdded(v) => Some(v.output_index),
            Self::ContentPartDone(v) => Some(v.output_index),
            Self::OutputTextDelta(v) => Some(v.output_index),
            Self::OutputTextDone(v) => Some(v.output_index),
            Self::OutputTextAnnotationAdded(v) => Some(v.output_index),
            Self::RefusalDelta(v) => Some(v.output_index),
            Self::RefusalDone(v) => Some(v.output_index),
            Self::ReasoningTextDelta(v) => Some(v.output_index),
            Self::ReasoningTextDone(v) => Some(v.output_index),
            Self::ReasoningSummaryPartAdded(v) => Some(v.output_index),
            Self::ReasoningSummaryPartDone(v) => Some(v.output_index),
            Self::ReasoningSummaryTextDelta(v) => Some(v.output_index),
            Self::ReasoningSummaryTextDone(v) => Some(v.output_index),
            Self::FunctionCallArgumentsDelta(v) => Some(v.output_index),
            Self::FunctionCallArgumentsDone(v) => Some(v.output_index),
            Self::CustomToolInputDelta(v) => Some(v.output_index),
            Self::CustomToolInputDone(v) => Some(v.output_index),
            Self::CodeInterpreterCodeDelta(v) => Some(v.output_index),
            Self::CodeInterpreterCodeDone(v) => Some(v.output_index),
            Self::ImagePartial(v) => Some(v.output_index),
            Self::ImageCall(v) | Self::ImageGenerating(v) | Self::ImageCompleted(v) => {
                Some(v.output_index)
            }
            Self::CodeInterpreterInProgress(v) => Some(v.output_index),
            Self::CodeInterpreterInterpreting(v) => Some(v.output_index),
            Self::CodeInterpreterCompleted(v) => Some(v.output_index),
            Self::FileSearchInProgress(v) => Some(v.output_index),
            Self::FileSearchSearching(v) => Some(v.output_index),
            Self::FileSearchCompleted(v) => Some(v.output_index),
            Self::WebSearchInProgress(v) => Some(v.output_index),
            Self::WebSearchSearching(v) => Some(v.output_index),
            Self::WebSearchCompleted(v) => Some(v.output_index),
            Self::McpArgumentsDelta(v) => Some(v.output_index),
            Self::McpArgumentsDone(v) => Some(v.output_index),
            Self::McpInProgress(v) => Some(v.output_index),
            Self::McpCompleted(v) => Some(v.output_index),
            Self::McpFailed(v) => Some(v.output_index),
            Self::McpListToolsInProgress(v) => Some(v.output_index),
            Self::McpListToolsCompleted(v) => Some(v.output_index),
            Self::McpListToolsFailed(v) => Some(v.output_index),
            _ => None,
        }
    }
}
