use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::create_response::response::ResponseBody as CreateResponseBody;
use crate::openai::create_response::types::{
    ResponseOutputItem, ResponseOutputRefusal, ResponseOutputText, ResponseReasoningTextContent,
    ResponseSummaryTextContent,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseStreamErrorPayload {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

impl ResponseStreamErrorPayload {
    pub fn code_or_type(&self) -> &str {
        self.code.as_deref().unwrap_or(self.type_.as_str())
    }
}

/// Stream event union documented by Responses API.
///
/// Each SSE `data:` line (except `[DONE]`) deserializes to one of these variants,
/// discriminated by the `type` field in JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseStreamEvent {
    #[serde(rename = "response.audio.delta")]
    AudioDelta {
        delta: String,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.audio.done")]
    AudioDone { sequence_number: u64 },
    #[serde(rename = "response.audio.transcript.delta")]
    AudioTranscriptDelta {
        delta: String,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.audio.transcript.done")]
    AudioTranscriptDone { sequence_number: u64 },

    #[serde(rename = "response.code_interpreter_call_code.delta")]
    CodeInterpreterCallCodeDelta {
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.code_interpreter_call_code.done")]
    CodeInterpreterCallCodeDone {
        code: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.code_interpreter_call.completed")]
    CodeInterpreterCallCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    CodeInterpreterCallInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    CodeInterpreterCallInterpreting {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.created")]
    Created {
        response: CreateResponseBody,
        sequence_number: u64,
    },
    #[serde(rename = "response.queued")]
    Queued {
        response: CreateResponseBody,
        sequence_number: u64,
    },
    #[serde(rename = "response.in_progress")]
    InProgress {
        response: CreateResponseBody,
        sequence_number: u64,
    },
    #[serde(rename = "response.failed")]
    Failed {
        response: CreateResponseBody,
        sequence_number: u64,
    },
    #[serde(rename = "response.incomplete")]
    Incomplete {
        response: CreateResponseBody,
        sequence_number: u64,
    },
    #[serde(rename = "response.completed")]
    Completed {
        response: CreateResponseBody,
        sequence_number: u64,
    },

    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        item: ResponseOutputItem,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        item: ResponseOutputItem,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        content_index: u64,
        item_id: String,
        output_index: u64,
        part: ResponseStreamContentPart,
        sequence_number: u64,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        content_index: u64,
        item_id: String,
        output_index: u64,
        part: ResponseStreamContentPart,
        sequence_number: u64,
    },

    #[serde(rename = "response.output_text.annotation.added")]
    OutputTextAnnotationAdded {
        annotation: Value,
        annotation_index: u64,
        content_index: u64,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        content_index: u64,
        delta: String,
        item_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<ResponseStreamTokenLogprob>>,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        content_index: u64,
        item_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<ResponseStreamTokenLogprob>>,
        output_index: u64,
        sequence_number: u64,
        text: String,
    },

    #[serde(rename = "response.refusal.delta")]
    RefusalDelta {
        content_index: u64,
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.refusal.done")]
    RefusalDone {
        content_index: u64,
        item_id: String,
        output_index: u64,
        refusal: String,
        sequence_number: u64,
    },

    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta {
        content_index: u64,
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone {
        content_index: u64,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        text: String,
    },

    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded {
        item_id: String,
        output_index: u64,
        part: ResponseSummaryTextContent,
        sequence_number: u64,
        summary_index: u64,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone {
        item_id: String,
        output_index: u64,
        part: ResponseSummaryTextContent,
        sequence_number: u64,
        summary_index: u64,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        summary_index: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        summary_index: u64,
        text: String,
    },

    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        arguments: String,
        item_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.file_search_call.in_progress")]
    FileSearchCallInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.file_search_call.searching")]
    FileSearchCallSearching {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.file_search_call.completed")]
    FileSearchCallCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.web_search_call.in_progress")]
    WebSearchCallInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.web_search_call.searching")]
    WebSearchCallSearching {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.web_search_call.completed")]
    WebSearchCallCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.image_generation_call.in_progress")]
    ImageGenerationCallInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.image_generation_call.generating")]
    ImageGenerationCallGenerating {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.image_generation_call.partial_image")]
    ImageGenerationCallPartialImage {
        item_id: String,
        output_index: u64,
        partial_image_b64: String,
        partial_image_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.image_generation_call.completed")]
    ImageGenerationCallCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpCallArgumentsDelta {
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpCallArgumentsDone {
        arguments: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.mcp_call.in_progress")]
    McpCallInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.mcp_call.completed")]
    McpCallCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.mcp_call.failed")]
    McpCallFailed {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.mcp_list_tools.in_progress")]
    McpListToolsInProgress {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.mcp_list_tools.completed")]
    McpListToolsCompleted {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },
    #[serde(rename = "response.mcp_list_tools.failed")]
    McpListToolsFailed {
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "response.custom_tool_call_input.delta")]
    CustomToolCallInputDelta {
        delta: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.custom_tool_call_input.done")]
    CustomToolCallInputDone {
        input: String,
        item_id: String,
        output_index: u64,
        sequence_number: u64,
    },

    #[serde(rename = "error")]
    Error {
        error: ResponseStreamErrorPayload,
        sequence_number: u64,
    },

    /// Undocumented SSE frame Codex ships while the upstream is still working
    /// (`event: keepalive\ndata: {"type":"keepalive","sequence_number":N}`).
    /// Not part of OpenAI's public Responses streaming spec but required to
    /// keep deserialization from failing mid-stream.
    #[serde(rename = "keepalive")]
    Keepalive { sequence_number: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseStreamContentPart {
    OutputText(ResponseOutputText),
    Refusal(ResponseOutputRefusal),
    ReasoningText(ResponseReasoningTextContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseStreamTokenLogprob {
    pub token: String,
    pub logprob: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<ResponseStreamTopLogprob>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseStreamTopLogprob {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprob: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Codex responses backend ships `{"type":"keepalive","sequence_number":N}`
    // mid-stream. Not in OpenAI's public spec, but real traffic includes it,
    // so rejecting the frame aborts the whole SSE with a 500.
    #[test]
    fn deserializes_codex_keepalive_frame() {
        let event: ResponseStreamEvent =
            serde_json::from_str(r#"{"type":"keepalive","sequence_number":5}"#)
                .expect("keepalive must deserialize");
        assert!(matches!(
            event,
            ResponseStreamEvent::Keepalive { sequence_number: 5 }
        ));
    }
}
