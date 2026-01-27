use serde::{Deserialize, Serialize};

use crate::openai::create_response::response::Response;
use crate::openai::create_response::types::{
    Annotation, OutputContent, OutputItem, ResponseLogProb, SummaryPart,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseStreamEvent {
    #[serde(rename = "response.audio.delta")]
    AudioDelta(ResponseAudioDeltaEvent),
    #[serde(rename = "response.audio.done")]
    AudioDone(ResponseAudioDoneEvent),
    #[serde(rename = "response.audio.transcript.delta")]
    AudioTranscriptDelta(ResponseAudioTranscriptDeltaEvent),
    #[serde(rename = "response.audio.transcript.done")]
    AudioTranscriptDone(ResponseAudioTranscriptDoneEvent),
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    CodeInterpreterCallCodeDelta(ResponseCodeInterpreterCallCodeDeltaEvent),
    #[serde(rename = "response.code_interpreter_call_code.done")]
    CodeInterpreterCallCodeDone(ResponseCodeInterpreterCallCodeDoneEvent),
    #[serde(rename = "response.code_interpreter_call.completed")]
    CodeInterpreterCallCompleted(ResponseCodeInterpreterCallCompletedEvent),
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    CodeInterpreterCallInProgress(ResponseCodeInterpreterCallInProgressEvent),
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    CodeInterpreterCallInterpreting(ResponseCodeInterpreterCallInterpretingEvent),
    #[serde(rename = "response.completed")]
    Completed(ResponseCompletedEvent),
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded(ResponseContentPartAddedEvent),
    #[serde(rename = "response.content_part.done")]
    ContentPartDone(ResponseContentPartDoneEvent),
    #[serde(rename = "response.created")]
    Created(ResponseCreatedEvent),
    #[serde(rename = "error")]
    Error(ResponseErrorEvent),
    #[serde(rename = "response.file_search_call.completed")]
    FileSearchCallCompleted(ResponseFileSearchCallCompletedEvent),
    #[serde(rename = "response.file_search_call.in_progress")]
    FileSearchCallInProgress(ResponseFileSearchCallInProgressEvent),
    #[serde(rename = "response.file_search_call.searching")]
    FileSearchCallSearching(ResponseFileSearchCallSearchingEvent),
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(ResponseFunctionCallArgumentsDeltaEvent),
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(ResponseFunctionCallArgumentsDoneEvent),
    #[serde(rename = "response.in_progress")]
    InProgress(ResponseInProgressEvent),
    #[serde(rename = "response.failed")]
    Failed(ResponseFailedEvent),
    #[serde(rename = "response.incomplete")]
    Incomplete(ResponseIncompleteEvent),
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded(ResponseOutputItemAddedEvent),
    #[serde(rename = "response.output_item.done")]
    OutputItemDone(ResponseOutputItemDoneEvent),
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded(ResponseReasoningSummaryPartAddedEvent),
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone(ResponseReasoningSummaryPartDoneEvent),
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta(ResponseReasoningSummaryTextDeltaEvent),
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone(ResponseReasoningSummaryTextDoneEvent),
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta(ResponseReasoningTextDeltaEvent),
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone(ResponseReasoningTextDoneEvent),
    #[serde(rename = "response.refusal.delta")]
    RefusalDelta(ResponseRefusalDeltaEvent),
    #[serde(rename = "response.refusal.done")]
    RefusalDone(ResponseRefusalDoneEvent),
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta(ResponseTextDeltaEvent),
    #[serde(rename = "response.output_text.done")]
    OutputTextDone(ResponseTextDoneEvent),
    #[serde(rename = "response.web_search_call.completed")]
    WebSearchCallCompleted(ResponseWebSearchCallCompletedEvent),
    #[serde(rename = "response.web_search_call.in_progress")]
    WebSearchCallInProgress(ResponseWebSearchCallInProgressEvent),
    #[serde(rename = "response.web_search_call.searching")]
    WebSearchCallSearching(ResponseWebSearchCallSearchingEvent),
    #[serde(rename = "response.image_generation_call.completed")]
    ImageGenCallCompleted(ResponseImageGenCallCompletedEvent),
    #[serde(rename = "response.image_generation_call.generating")]
    ImageGenCallGenerating(ResponseImageGenCallGeneratingEvent),
    #[serde(rename = "response.image_generation_call.in_progress")]
    ImageGenCallInProgress(ResponseImageGenCallInProgressEvent),
    #[serde(rename = "response.image_generation_call.partial_image")]
    ImageGenCallPartialImage(ResponseImageGenCallPartialImageEvent),
    #[serde(rename = "response.mcp_call_arguments.delta")]
    MCPCallArgumentsDelta(ResponseMCPCallArgumentsDeltaEvent),
    #[serde(rename = "response.mcp_call_arguments.done")]
    MCPCallArgumentsDone(ResponseMCPCallArgumentsDoneEvent),
    #[serde(rename = "response.mcp_call.completed")]
    MCPCallCompleted(ResponseMCPCallCompletedEvent),
    #[serde(rename = "response.mcp_call.failed")]
    MCPCallFailed(ResponseMCPCallFailedEvent),
    #[serde(rename = "response.mcp_call.in_progress")]
    MCPCallInProgress(ResponseMCPCallInProgressEvent),
    #[serde(rename = "response.mcp_list_tools.completed")]
    MCPListToolsCompleted(ResponseMCPListToolsCompletedEvent),
    #[serde(rename = "response.mcp_list_tools.failed")]
    MCPListToolsFailed(ResponseMCPListToolsFailedEvent),
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    MCPListToolsInProgress(ResponseMCPListToolsInProgressEvent),
    #[serde(rename = "response.output_text.annotation.added")]
    OutputTextAnnotationAdded(ResponseOutputTextAnnotationAddedEvent),
    #[serde(rename = "response.queued")]
    Queued(ResponseQueuedEvent),
    #[serde(rename = "response.custom_tool_call_input.delta")]
    CustomToolCallInputDelta(ResponseCustomToolCallInputDeltaEvent),
    #[serde(rename = "response.custom_tool_call_input.done")]
    CustomToolCallInputDone(ResponseCustomToolCallInputDoneEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseAudioDeltaEvent {
    pub sequence_number: i64,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseAudioDoneEvent {
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseAudioTranscriptDeltaEvent {
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseAudioTranscriptDoneEvent {
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCodeInterpreterCallCodeDeltaEvent {
    pub output_index: i64,
    pub item_id: String,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCodeInterpreterCallCodeDoneEvent {
    pub output_index: i64,
    pub item_id: String,
    pub code: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCodeInterpreterCallCompletedEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCodeInterpreterCallInProgressEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCodeInterpreterCallInterpretingEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCompletedEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseContentPartAddedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub part: OutputContent,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseContentPartDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub part: OutputContent,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCreatedEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseErrorEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFileSearchCallCompletedEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFileSearchCallInProgressEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFileSearchCallSearchingEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFunctionCallArgumentsDeltaEvent {
    pub item_id: String,
    pub output_index: i64,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFunctionCallArgumentsDoneEvent {
    pub item_id: String,
    pub name: String,
    pub output_index: i64,
    pub arguments: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseInProgressEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFailedEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseIncompleteEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseOutputItemAddedEvent {
    pub output_index: i64,
    pub item: OutputItem,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseOutputItemDoneEvent {
    pub output_index: i64,
    pub item: OutputItem,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningSummaryPartAddedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub summary_index: i64,
    pub part: SummaryPart,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningSummaryPartDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub summary_index: i64,
    pub part: SummaryPart,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningSummaryTextDeltaEvent {
    pub item_id: String,
    pub output_index: i64,
    pub summary_index: i64,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningSummaryTextDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub summary_index: i64,
    pub text: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningTextDeltaEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseReasoningTextDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub text: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseRefusalDeltaEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseRefusalDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub refusal: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseTextDeltaEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub delta: String,
    pub sequence_number: i64,
    /// Only present when logprobs are enabled (not enforced here).
    #[serde(default)]
    pub logprobs: Vec<ResponseLogProb>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseTextDoneEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub text: String,
    pub sequence_number: i64,
    /// Only present when logprobs are enabled (not enforced here).
    #[serde(default)]
    pub logprobs: Vec<ResponseLogProb>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseWebSearchCallCompletedEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseWebSearchCallInProgressEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseWebSearchCallSearchingEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseImageGenCallCompletedEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseImageGenCallGeneratingEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseImageGenCallInProgressEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseImageGenCallPartialImageEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
    pub partial_image_index: i64,
    pub partial_image_b64: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPCallArgumentsDeltaEvent {
    pub output_index: i64,
    pub item_id: String,
    pub delta: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPCallArgumentsDoneEvent {
    pub output_index: i64,
    pub item_id: String,
    pub arguments: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPCallCompletedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPCallFailedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPCallInProgressEvent {
    pub output_index: i64,
    pub item_id: String,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPListToolsCompletedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPListToolsFailedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseMCPListToolsInProgressEvent {
    pub item_id: String,
    pub output_index: i64,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseOutputTextAnnotationAddedEvent {
    pub item_id: String,
    pub output_index: i64,
    pub content_index: i64,
    pub annotation_index: i64,
    pub annotation: Annotation,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseQueuedEvent {
    pub response: Response,
    pub sequence_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCustomToolCallInputDeltaEvent {
    pub sequence_number: i64,
    pub output_index: i64,
    pub item_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseCustomToolCallInputDoneEvent {
    pub sequence_number: i64,
    pub output_index: i64,
    pub item_id: String,
    pub input: String,
}
