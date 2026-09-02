use serde::{Deserialize, Serialize};

use super::super::{ResponseInjectCreatedEvent, ResponseInjectFailedEvent};
use super::payloads::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownResponseStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated(ResponseLifecycleEvent),
    #[serde(rename = "response.in_progress")]
    ResponseInProgress(ResponseLifecycleEvent),
    #[serde(rename = "response.completed")]
    ResponseCompleted(ResponseLifecycleEvent),
    #[serde(rename = "response.failed")]
    ResponseFailed(ResponseLifecycleEvent),
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete(ResponseLifecycleEvent),
    #[serde(rename = "response.queued")]
    ResponseQueued(ResponseLifecycleEvent),
    #[serde(rename = "response.inject.created")]
    ResponseInjectCreated(ResponseInjectCreatedEvent),
    #[serde(rename = "response.inject.failed")]
    ResponseInjectFailed(ResponseInjectFailedEvent),
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded(ResponseOutputItemEvent),
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone(ResponseOutputItemEvent),
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded(ResponseContentPartEvent),
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone(ResponseContentPartEvent),
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta(ResponseOutputTextDeltaEvent),
    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone(ResponseOutputTextDoneEvent),
    #[serde(rename = "response.output_text.annotation.added")]
    ResponseOutputTextAnnotationAdded(ResponseOutputTextAnnotationEvent),
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta(ResponseItemStringDeltaEvent),
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone(ResponseFunctionCallArgumentsDoneEvent),
    #[serde(rename = "response.custom_tool_call_input.delta")]
    ResponseCustomToolCallInputDelta(ResponseItemStringDeltaEvent),
    #[serde(rename = "response.custom_tool_call_input.done")]
    ResponseCustomToolCallInputDone(ResponseCustomToolCallInputDoneEvent),
    #[serde(rename = "response.refusal.delta")]
    ResponseRefusalDelta(ResponseContentDeltaEvent),
    #[serde(rename = "response.refusal.done")]
    ResponseRefusalDone(ResponseRefusalDoneEvent),
    #[serde(rename = "response.reasoning_summary_part.added")]
    ResponseReasoningSummaryPartAdded(ResponseReasoningSummaryPartAddedEvent),
    #[serde(rename = "response.reasoning_summary_part.done")]
    ResponseReasoningSummaryPartDone(ResponseReasoningSummaryPartDoneEvent),
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ResponseReasoningSummaryTextDelta(ResponseReasoningSummaryTextDeltaEvent),
    #[serde(rename = "response.reasoning_summary_text.done")]
    ResponseReasoningSummaryTextDone(ResponseReasoningSummaryTextDoneEvent),
    #[serde(rename = "response.reasoning_text.delta")]
    ResponseReasoningTextDelta(ResponseContentDeltaEvent),
    #[serde(rename = "response.reasoning_text.done")]
    ResponseReasoningTextDone(ResponseContentTextDoneEvent),
    #[serde(rename = "response.audio.delta")]
    ResponseAudioDelta(ResponseAudioDeltaEvent),
    #[serde(rename = "response.audio.done")]
    ResponseAudioDone(ResponseSequenceEvent),
    #[serde(rename = "response.audio.transcript.delta")]
    ResponseAudioTranscriptDelta(ResponseAudioDeltaEvent),
    #[serde(rename = "response.audio.transcript.done")]
    ResponseAudioTranscriptDone(ResponseSequenceEvent),
    #[serde(rename = "response.image_generation_call.completed")]
    ResponseImageGenerationCallCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.image_generation_call.generating")]
    ResponseImageGenerationCallGenerating(ResponseToolProgressEvent),
    #[serde(rename = "response.image_generation_call.in_progress")]
    ResponseImageGenerationCallInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.image_generation_call.partial_image")]
    ResponseImageGenerationCallPartialImage(ResponseImagePartialEvent),
    #[serde(rename = "response.file_search_call.in_progress")]
    ResponseFileSearchCallInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.file_search_call.searching")]
    ResponseFileSearchCallSearching(ResponseToolProgressEvent),
    #[serde(rename = "response.file_search_call.completed")]
    ResponseFileSearchCallCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.web_search_call.in_progress")]
    ResponseWebSearchCallInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.web_search_call.searching")]
    ResponseWebSearchCallSearching(ResponseToolProgressEvent),
    #[serde(rename = "response.web_search_call.completed")]
    ResponseWebSearchCallCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    ResponseCodeInterpreterCallInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    ResponseCodeInterpreterCallInterpreting(ResponseToolProgressEvent),
    #[serde(rename = "response.code_interpreter_call.completed")]
    ResponseCodeInterpreterCallCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    ResponseCodeInterpreterCallCodeDelta(ResponseItemStringDeltaEvent),
    #[serde(rename = "response.code_interpreter_call_code.done")]
    ResponseCodeInterpreterCallCodeDone(ResponseCodeInterpreterCallCodeDoneEvent),
    #[serde(rename = "response.mcp_call_arguments.delta")]
    ResponseMcpCallArgumentsDelta(ResponseItemStringDeltaEvent),
    #[serde(rename = "response.mcp_call_arguments.done")]
    ResponseMcpCallArgumentsDone(ResponseMcpCallArgumentsDoneEvent),
    #[serde(rename = "response.mcp_call.in_progress")]
    ResponseMcpCallInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.mcp_call.completed")]
    ResponseMcpCallCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.mcp_call.failed")]
    ResponseMcpCallFailed(ResponseToolProgressEvent),
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    ResponseMcpListToolsInProgress(ResponseToolProgressEvent),
    #[serde(rename = "response.mcp_list_tools.completed")]
    ResponseMcpListToolsCompleted(ResponseToolProgressEvent),
    #[serde(rename = "response.mcp_list_tools.failed")]
    ResponseMcpListToolsFailed(ResponseToolProgressEvent),
    #[serde(rename = "error")]
    Error(ResponseErrorEvent),
}
