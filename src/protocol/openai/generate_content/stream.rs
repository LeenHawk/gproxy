use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;
use super::{
    ResponseContentPart, ResponseObject, ResponseOutputItem, ResponseReasoningSummaryPart,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ResponseStreamEvent {
    Known(KnownResponseStreamEvent),
    Unknown(UnknownResponseStreamEvent),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownResponseStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseStreamEventType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
