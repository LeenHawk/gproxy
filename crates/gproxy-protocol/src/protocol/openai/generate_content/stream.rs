use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;
use super::{
    ResponseContentPart, ResponseObject, ResponseOutputItem, ResponseReasoningSummaryPart,
};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ResponseStreamEvent {
    Known(KnownResponseStreamEvent),
    Unknown(UnknownResponseStreamEvent),
}

impl ResponseStreamEvent {
    /// SSE event name: the wire `type` of this event, if any.
    pub fn event_name(&self) -> Option<&str> {
        match self {
            Self::Known(event) => Some(event.event_name()),
            Self::Unknown(event) => event.type_.as_ref().map(ResponseStreamEventType::as_str),
        }
    }
}

impl Serialize for ResponseStreamEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Known(event) => event.serialize(serializer),
            Self::Unknown(event) => event.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ResponseStreamEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let Some(type_name) = value.get("type").and_then(Value::as_str) else {
            return serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom);
        };

        let event_type =
            serde_json::from_value::<ResponseStreamEventType>(Value::String(type_name.to_owned()))
                .map_err(de::Error::custom)?;

        match event_type {
            ResponseStreamEventType::Known(_) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            ResponseStreamEventType::Unknown(_) => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum KnownResponseStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.queued")]
    ResponseQueued {
        response: Box<ResponseObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded {
        item: Box<ResponseOutputItem>,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone {
        item: Box<ResponseOutputItem>,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded {
        content_index: u32,
        item_id: String,
        output_index: u32,
        part: ResponseContentPart,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone {
        content_index: u32,
        item_id: String,
        output_index: u32,
        part: ResponseContentPart,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta {
        content_index: u32,
        delta: String,
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<StreamTokenLogprob>>,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone {
        content_index: u32,
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<StreamTokenLogprob>>,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_text.annotation.added")]
    ResponseOutputTextAnnotationAdded {
        annotation: Value,
        annotation_index: u32,
        content_index: u32,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone {
        arguments: String,
        item_id: String,
        // The ChatGPT Codex backend can omit this field. Keep the public wire
        // type stable and let stateful consumers recover it from the matching
        // output item.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.custom_tool_call_input.delta")]
    ResponseCustomToolCallInputDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.custom_tool_call_input.done")]
    ResponseCustomToolCallInputDone {
        input: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.refusal.delta")]
    ResponseRefusalDelta {
        content_index: u32,
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.refusal.done")]
    ResponseRefusalDone {
        content_index: u32,
        item_id: String,
        output_index: u32,
        refusal: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_summary_part.added")]
    ResponseReasoningSummaryPartAdded {
        item_id: String,
        output_index: u32,
        part: ResponseReasoningSummaryPart,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        summary_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ResponseReasoningSummaryPartDone {
        item_id: String,
        output_index: u32,
        part: ResponseReasoningSummaryPart,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        summary_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ResponseReasoningSummaryTextDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        summary_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ResponseReasoningSummaryTextDone {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        summary_index: u32,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_text.delta")]
    ResponseReasoningTextDelta {
        content_index: u32,
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.reasoning_text.done")]
    ResponseReasoningTextDone {
        content_index: u32,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.audio.delta")]
    ResponseAudioDelta {
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.audio.done")]
    ResponseAudioDone {
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.audio.transcript.delta")]
    ResponseAudioTranscriptDelta {
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.audio.transcript.done")]
    ResponseAudioTranscriptDone {
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.image_generation_call.completed")]
    ResponseImageGenerationCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.image_generation_call.generating")]
    ResponseImageGenerationCallGenerating {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.image_generation_call.in_progress")]
    ResponseImageGenerationCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.image_generation_call.partial_image")]
    ResponseImageGenerationCallPartialImage {
        item_id: String,
        output_index: u32,
        partial_image_b64: String,
        partial_image_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.file_search_call.in_progress")]
    ResponseFileSearchCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.file_search_call.searching")]
    ResponseFileSearchCallSearching {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.file_search_call.completed")]
    ResponseFileSearchCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.web_search_call.in_progress")]
    ResponseWebSearchCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.web_search_call.searching")]
    ResponseWebSearchCallSearching {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.web_search_call.completed")]
    ResponseWebSearchCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    ResponseCodeInterpreterCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    ResponseCodeInterpreterCallInterpreting {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.code_interpreter_call.completed")]
    ResponseCodeInterpreterCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    ResponseCodeInterpreterCallCodeDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.code_interpreter_call_code.done")]
    ResponseCodeInterpreterCallCodeDone {
        code: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call_arguments.delta")]
    ResponseMcpCallArgumentsDelta {
        delta: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call_arguments.done")]
    ResponseMcpCallArgumentsDone {
        arguments: String,
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.in_progress")]
    ResponseMcpCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.completed")]
    ResponseMcpCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.failed")]
    ResponseMcpCallFailed {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    ResponseMcpListToolsInProgress {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_list_tools.completed")]
    ResponseMcpListToolsCompleted {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_list_tools.failed")]
    ResponseMcpListToolsFailed {
        item_id: String,
        output_index: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
        param: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

impl KnownResponseStreamEvent {
    /// Typed event type of this variant, mirroring the serde `type` tag.
    pub fn event_type(&self) -> ResponseStreamEventTypeKnown {
        use ResponseStreamEventTypeKnown as T;
        match self {
            Self::ResponseCreated { .. } => T::ResponseCreated,
            Self::ResponseInProgress { .. } => T::ResponseInProgress,
            Self::ResponseCompleted { .. } => T::ResponseCompleted,
            Self::ResponseFailed { .. } => T::ResponseFailed,
            Self::ResponseIncomplete { .. } => T::ResponseIncomplete,
            Self::ResponseQueued { .. } => T::ResponseQueued,
            Self::ResponseOutputItemAdded { .. } => T::ResponseOutputItemAdded,
            Self::ResponseOutputItemDone { .. } => T::ResponseOutputItemDone,
            Self::ResponseContentPartAdded { .. } => T::ResponseContentPartAdded,
            Self::ResponseContentPartDone { .. } => T::ResponseContentPartDone,
            Self::ResponseOutputTextDelta { .. } => T::ResponseOutputTextDelta,
            Self::ResponseOutputTextDone { .. } => T::ResponseOutputTextDone,
            Self::ResponseOutputTextAnnotationAdded { .. } => T::ResponseOutputTextAnnotationAdded,
            Self::ResponseFunctionCallArgumentsDelta { .. } => {
                T::ResponseFunctionCallArgumentsDelta
            }
            Self::ResponseFunctionCallArgumentsDone { .. } => T::ResponseFunctionCallArgumentsDone,
            Self::ResponseCustomToolCallInputDelta { .. } => T::ResponseCustomToolCallInputDelta,
            Self::ResponseCustomToolCallInputDone { .. } => T::ResponseCustomToolCallInputDone,
            Self::ResponseRefusalDelta { .. } => T::ResponseRefusalDelta,
            Self::ResponseRefusalDone { .. } => T::ResponseRefusalDone,
            Self::ResponseReasoningSummaryPartAdded { .. } => T::ResponseReasoningSummaryPartAdded,
            Self::ResponseReasoningSummaryPartDone { .. } => T::ResponseReasoningSummaryPartDone,
            Self::ResponseReasoningSummaryTextDelta { .. } => T::ResponseReasoningSummaryTextDelta,
            Self::ResponseReasoningSummaryTextDone { .. } => T::ResponseReasoningSummaryTextDone,
            Self::ResponseReasoningTextDelta { .. } => T::ResponseReasoningTextDelta,
            Self::ResponseReasoningTextDone { .. } => T::ResponseReasoningTextDone,
            Self::ResponseAudioDelta { .. } => T::ResponseAudioDelta,
            Self::ResponseAudioDone { .. } => T::ResponseAudioDone,
            Self::ResponseAudioTranscriptDelta { .. } => T::ResponseAudioTranscriptDelta,
            Self::ResponseAudioTranscriptDone { .. } => T::ResponseAudioTranscriptDone,
            Self::ResponseImageGenerationCallCompleted { .. } => {
                T::ResponseImageGenerationCallCompleted
            }
            Self::ResponseImageGenerationCallGenerating { .. } => {
                T::ResponseImageGenerationCallGenerating
            }
            Self::ResponseImageGenerationCallInProgress { .. } => {
                T::ResponseImageGenerationCallInProgress
            }
            Self::ResponseImageGenerationCallPartialImage { .. } => {
                T::ResponseImageGenerationCallPartialImage
            }
            Self::ResponseFileSearchCallInProgress { .. } => T::ResponseFileSearchCallInProgress,
            Self::ResponseFileSearchCallSearching { .. } => T::ResponseFileSearchCallSearching,
            Self::ResponseFileSearchCallCompleted { .. } => T::ResponseFileSearchCallCompleted,
            Self::ResponseWebSearchCallInProgress { .. } => T::ResponseWebSearchCallInProgress,
            Self::ResponseWebSearchCallSearching { .. } => T::ResponseWebSearchCallSearching,
            Self::ResponseWebSearchCallCompleted { .. } => T::ResponseWebSearchCallCompleted,
            Self::ResponseCodeInterpreterCallInProgress { .. } => {
                T::ResponseCodeInterpreterCallInProgress
            }
            Self::ResponseCodeInterpreterCallInterpreting { .. } => {
                T::ResponseCodeInterpreterCallInterpreting
            }
            Self::ResponseCodeInterpreterCallCompleted { .. } => {
                T::ResponseCodeInterpreterCallCompleted
            }
            Self::ResponseCodeInterpreterCallCodeDelta { .. } => {
                T::ResponseCodeInterpreterCallCodeDelta
            }
            Self::ResponseCodeInterpreterCallCodeDone { .. } => {
                T::ResponseCodeInterpreterCallCodeDone
            }
            Self::ResponseMcpCallArgumentsDelta { .. } => T::ResponseMcpCallArgumentsDelta,
            Self::ResponseMcpCallArgumentsDone { .. } => T::ResponseMcpCallArgumentsDone,
            Self::ResponseMcpCallInProgress { .. } => T::ResponseMcpCallInProgress,
            Self::ResponseMcpCallCompleted { .. } => T::ResponseMcpCallCompleted,
            Self::ResponseMcpCallFailed { .. } => T::ResponseMcpCallFailed,
            Self::ResponseMcpListToolsInProgress { .. } => T::ResponseMcpListToolsInProgress,
            Self::ResponseMcpListToolsCompleted { .. } => T::ResponseMcpListToolsCompleted,
            Self::ResponseMcpListToolsFailed { .. } => T::ResponseMcpListToolsFailed,
            Self::Error { .. } => T::Error,
        }
    }

    /// SSE event name: the exact serde rename of this variant.
    pub fn event_name(&self) -> &'static str {
        self.event_type().as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct UnknownResponseStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseStreamEventType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `event_name()` must equal the serialized `type` tag.
    #[test]
    fn event_name_matches_serialized_type_tag() {
        let events: Vec<ResponseStreamEvent> = [
            r#"{"type":"response.output_text.delta","content_index":0,"delta":"hi","item_id":"msg_0","output_index":0}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"id":"fc_1","type":"function_call","call_id":"c1","name":"f","arguments":"{}"}}"#,
            r#"{"type":"response.function_call_arguments.done","arguments":"{}","item_id":"fc_1","name":"f","output_index":0}"#,
            r#"{"type":"response.completed","response":{"id":"r","object":"response","created_at":0,"output":[]}}"#,
            r#"{"type":"response.some_future_event","sequence_number":1}"#,
        ]
        .iter()
        .map(|raw| serde_json::from_str(raw).unwrap())
        .collect();
        for event in events {
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(
                event.event_name(),
                value.get("type").and_then(Value::as_str),
                "{value}"
            );
        }
    }
}
