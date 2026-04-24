use std::collections::BTreeMap;

use crate::gemini::count_tokens::types::{GeminiContentRole, GeminiFunctionCall, GeminiPart};
use crate::gemini::generate_content::response::ResponseBody as GeminiGenerateContentResponseBody;
use crate::gemini::generate_content::types::{
    GeminiBlockReason, GeminiCandidate, GeminiContent, GeminiFinishReason, GeminiPromptFeedback,
    GeminiUsageMetadata,
};
use crate::openai::count_tokens::types::{
    ResponseCustomToolCallOutputContent, ResponseFunctionCallOutputContent, ResponseInputContent,
};
use crate::openai::create_response::response::ResponseBody as OpenAiCreateResponseBody;
use crate::openai::create_response::stream::{ResponseStreamContentPart, ResponseStreamEvent};
use crate::openai::create_response::types::{ResponseIncompleteReason, ResponseOutputItem};
use crate::transform::gemini::stream_generate_content::utils::parse_json_object_or_empty;

#[derive(Debug, Clone, Default)]
struct FunctionCallState {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct OpenAiResponseToGeminiStream {
    response_id: Option<String>,
    model_version: Option<String>,
    usage_metadata: Option<GeminiUsageMetadata>,
    function_calls: BTreeMap<String, FunctionCallState>,
    finished: bool,
}

impl OpenAiResponseToGeminiStream {
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    fn apply_response_state(&mut self, response: &OpenAiCreateResponseBody) {
        self.response_id = Some(response.id.clone());
        self.model_version = Some(response.model.clone());
        self.usage_metadata = response.usage.as_ref().map(|usage| GeminiUsageMetadata {
            prompt_token_count: Some(usage.input_tokens),
            cached_content_token_count: Some(usage.input_tokens_details.cached_tokens),
            candidates_token_count: Some(usage.output_tokens),
            thoughts_token_count: Some(usage.output_tokens_details.reasoning_tokens),
            total_token_count: Some(usage.total_tokens),
            ..GeminiUsageMetadata::default()
        });
    }

    fn finish_reason_from_incomplete_reason(
        reason: Option<&ResponseIncompleteReason>,
    ) -> GeminiFinishReason {
        match reason {
            Some(ResponseIncompleteReason::MaxOutputTokens) => GeminiFinishReason::MaxTokens,
            Some(ResponseIncompleteReason::ContentFilter) => GeminiFinishReason::Safety,
            None => GeminiFinishReason::Stop,
        }
    }

    fn chunk_from_parts(
        &self,
        parts: Vec<GeminiPart>,
        finish_reason: Option<GeminiFinishReason>,
        prompt_feedback: Option<GeminiPromptFeedback>,
    ) -> GeminiGenerateContentResponseBody {
        GeminiGenerateContentResponseBody {
            candidates: Some(vec![GeminiCandidate {
                content: Some(GeminiContent {
                    parts,
                    role: Some(GeminiContentRole::Model),
                }),
                finish_reason,
                index: Some(0),
                ..GeminiCandidate::default()
            }]),
            prompt_feedback,
            usage_metadata: self.usage_metadata.clone(),
            model_version: self.model_version.clone(),
            response_id: self.response_id.clone(),
            model_status: None,
        }
    }

    fn text_chunk(&self, text: String) -> Option<GeminiGenerateContentResponseBody> {
        if text.is_empty() {
            None
        } else {
            Some(self.chunk_from_parts(
                vec![GeminiPart {
                    text: Some(text),
                    ..GeminiPart::default()
                }],
                None,
                None,
            ))
        }
    }

    fn thinking_chunk(
        &self,
        signature: String,
        thinking: String,
    ) -> Option<GeminiGenerateContentResponseBody> {
        if thinking.is_empty() {
            None
        } else {
            Some(self.chunk_from_parts(
                vec![GeminiPart {
                    thought: Some(true),
                    thought_signature: Some(signature),
                    text: Some(thinking),
                    ..GeminiPart::default()
                }],
                None,
                None,
            ))
        }
    }

    fn function_call_chunk(
        &self,
        id: String,
        name: String,
        arguments: String,
    ) -> GeminiGenerateContentResponseBody {
        self.chunk_from_parts(
            vec![GeminiPart {
                function_call: Some(GeminiFunctionCall {
                    id: Some(id),
                    name,
                    args: Some(parse_json_object_or_empty(&arguments)),
                }),
                ..GeminiPart::default()
            }],
            None,
            None,
        )
    }

    fn input_content_to_text(items: Vec<ResponseInputContent>) -> String {
        items
            .into_iter()
            .filter_map(|item| match item {
                ResponseInputContent::Text(text) => Some(text.text),
                ResponseInputContent::Image(image) => {
                    if let Some(url) = image.image_url {
                        Some(url)
                    } else {
                        image.file_id.map(|file_id| format!("file:{file_id}"))
                    }
                }
                ResponseInputContent::File(file) => {
                    if let Some(data) = file.file_data {
                        Some(data)
                    } else if let Some(url) = file.file_url {
                        Some(url)
                    } else if let Some(file_id) = file.file_id {
                        Some(format!("file:{file_id}"))
                    } else {
                        file.filename
                    }
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn map_output_item(
        &self,
        item: ResponseOutputItem,
        out: &mut Vec<GeminiGenerateContentResponseBody>,
    ) {
        match item {
            ResponseOutputItem::Message(message) => {
                for content in message.content {
                    match content {
                        crate::openai::count_tokens::types::ResponseOutputContent::Text(text) => {
                            if let Some(chunk) = self.text_chunk(text.text) {
                                out.push(chunk);
                            }
                        }
                        crate::openai::count_tokens::types::ResponseOutputContent::Refusal(
                            refusal,
                        ) => {
                            if let Some(chunk) = self.text_chunk(refusal.refusal) {
                                out.push(chunk);
                            }
                        }
                    }
                }
            }
            ResponseOutputItem::FunctionToolCall(call) => {
                out.push(self.function_call_chunk(
                    call.id.unwrap_or(call.call_id),
                    call.name,
                    call.arguments,
                ));
            }
            ResponseOutputItem::CustomToolCall(call) => {
                out.push(self.function_call_chunk(
                    call.id.unwrap_or(call.call_id),
                    call.name,
                    call.input,
                ));
            }
            ResponseOutputItem::ReasoningItem(item) => {
                if let Some(signature) = item.id.filter(|id| !id.is_empty()) {
                    for summary in item.summary {
                        if let Some(chunk) = self.thinking_chunk(signature.clone(), summary.text) {
                            out.push(chunk);
                        }
                    }
                    if let Some(content) = item.content {
                        for reasoning in content {
                            if let Some(chunk) =
                                self.thinking_chunk(signature.clone(), reasoning.text)
                            {
                                out.push(chunk);
                            }
                        }
                    }
                    if let Some(encrypted_content) = item.encrypted_content
                        && let Some(chunk) = self.thinking_chunk(signature, encrypted_content)
                    {
                        out.push(chunk);
                    }
                }
            }
            ResponseOutputItem::FunctionCallOutput(call) => {
                let text = match call.output {
                    ResponseFunctionCallOutputContent::Text(text) => text,
                    ResponseFunctionCallOutputContent::Content(items) => {
                        Self::input_content_to_text(items)
                    }
                };
                if let Some(chunk) = self.text_chunk(text) {
                    out.push(chunk);
                }
            }
            ResponseOutputItem::CustomToolCallOutput(call) => {
                let text = match call.output {
                    ResponseCustomToolCallOutputContent::Text(text) => text,
                    ResponseCustomToolCallOutputContent::Content(items) => {
                        Self::input_content_to_text(items)
                    }
                };
                if let Some(chunk) = self.text_chunk(text) {
                    out.push(chunk);
                }
            }
            ResponseOutputItem::ShellCallOutput(call) => {
                let text = call
                    .output
                    .into_iter()
                    .map(|entry| format!("stdout: {}\nstderr: {}", entry.stdout, entry.stderr))
                    .collect::<Vec<_>>()
                    .join("\n");
                if let Some(chunk) = self.text_chunk(text) {
                    out.push(chunk);
                }
            }
            ResponseOutputItem::LocalShellCallOutput(call) => {
                if let Some(chunk) = self.text_chunk(call.output) {
                    out.push(chunk);
                }
            }
            ResponseOutputItem::McpCall(call) => {
                if let Some(output) = call.output
                    && let Some(chunk) = self.text_chunk(output)
                {
                    out.push(chunk);
                }
                if let Some(error) = call.error
                    && let Some(chunk) = self.text_chunk(error)
                {
                    out.push(chunk);
                }
            }
            ResponseOutputItem::ImageGenerationCall(call) => {
                if let Some(chunk) = call.result.and_then(|r| self.text_chunk(r)) {
                    out.push(chunk);
                }
            }
            _ => {}
        }
    }

    pub fn on_stream_event(
        &mut self,
        event: ResponseStreamEvent,
        out: &mut Vec<GeminiGenerateContentResponseBody>,
    ) {
        if self.finished {
            return;
        }

        match event {
            ResponseStreamEvent::Created { response, .. }
            | ResponseStreamEvent::Queued { response, .. }
            | ResponseStreamEvent::InProgress { response, .. } => {
                self.apply_response_state(&response);
            }
            ResponseStreamEvent::Completed { response, .. }
            | ResponseStreamEvent::Incomplete { response, .. } => {
                self.apply_response_state(&response);
                let reason = Self::finish_reason_from_incomplete_reason(
                    response
                        .incomplete_details
                        .as_ref()
                        .and_then(|details| details.reason.as_ref()),
                );
                let prompt_feedback = if matches!(reason, GeminiFinishReason::Safety) {
                    Some(GeminiPromptFeedback {
                        block_reason: Some(GeminiBlockReason::Safety),
                        safety_ratings: None,
                    })
                } else {
                    None
                };
                out.push(self.chunk_from_parts(Vec::new(), Some(reason), prompt_feedback));
            }
            ResponseStreamEvent::Failed { response, .. } => {
                self.apply_response_state(&response);
                if let Some(error) = response.error
                    && let Some(chunk) = self.text_chunk(error.message)
                {
                    out.push(chunk);
                }
                out.push(self.chunk_from_parts(
                    Vec::new(),
                    Some(GeminiFinishReason::Safety),
                    Some(GeminiPromptFeedback {
                        block_reason: Some(GeminiBlockReason::Safety),
                        safety_ratings: None,
                    }),
                ));
            }
            ResponseStreamEvent::OutputTextDelta { delta, .. }
            | ResponseStreamEvent::OutputTextDone { text: delta, .. } => {
                if let Some(chunk) = self.text_chunk(delta) {
                    out.push(chunk);
                }
            }
            ResponseStreamEvent::RefusalDelta { delta, .. }
            | ResponseStreamEvent::RefusalDone { refusal: delta, .. } => {
                if let Some(chunk) = self.text_chunk(delta) {
                    out.push(chunk);
                }
            }
            ResponseStreamEvent::ReasoningTextDelta { item_id, delta, .. }
            | ResponseStreamEvent::ReasoningTextDone {
                item_id,
                text: delta,
                ..
            }
            | ResponseStreamEvent::ReasoningSummaryTextDelta { item_id, delta, .. }
            | ResponseStreamEvent::ReasoningSummaryTextDone {
                item_id,
                text: delta,
                ..
            } => {
                if let Some(chunk) = self.thinking_chunk(item_id, delta) {
                    out.push(chunk);
                }
            }
            ResponseStreamEvent::FunctionCallArgumentsDelta { item_id, delta, .. } => {
                let snapshot = {
                    let entry = self
                        .function_calls
                        .entry(item_id.clone())
                        .or_insert_with(|| FunctionCallState {
                            name: "function".to_string(),
                            arguments: String::new(),
                        });
                    entry.arguments.push_str(&delta);
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::FunctionCallArgumentsDone {
                item_id,
                name,
                arguments,
                ..
            } => {
                let snapshot = {
                    let entry = self.function_calls.entry(item_id.clone()).or_default();
                    if let Some(name) = name
                        && !name.is_empty()
                    {
                        entry.name = name;
                    }
                    entry.arguments = arguments;
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::McpCallArgumentsDelta { item_id, delta, .. } => {
                let snapshot = {
                    let entry = self
                        .function_calls
                        .entry(item_id.clone())
                        .or_insert_with(|| FunctionCallState {
                            name: "mcp_call".to_string(),
                            arguments: String::new(),
                        });
                    entry.arguments.push_str(&delta);
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::McpCallArgumentsDone {
                item_id, arguments, ..
            } => {
                let snapshot = {
                    let entry = self
                        .function_calls
                        .entry(item_id.clone())
                        .or_insert_with(|| FunctionCallState {
                            name: "mcp_call".to_string(),
                            arguments: String::new(),
                        });
                    entry.arguments = arguments;
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::CustomToolCallInputDelta { item_id, delta, .. } => {
                let snapshot = {
                    let entry = self
                        .function_calls
                        .entry(item_id.clone())
                        .or_insert_with(|| FunctionCallState {
                            name: "custom_tool".to_string(),
                            arguments: String::new(),
                        });
                    entry.arguments.push_str(&delta);
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::CustomToolCallInputDone { item_id, input, .. } => {
                let snapshot = {
                    let entry = self
                        .function_calls
                        .entry(item_id.clone())
                        .or_insert_with(|| FunctionCallState {
                            name: "custom_tool".to_string(),
                            arguments: String::new(),
                        });
                    entry.arguments = input;
                    (entry.name.clone(), entry.arguments.clone())
                };
                out.push(self.function_call_chunk(item_id, snapshot.0, snapshot.1));
            }
            ResponseStreamEvent::ContentPartAdded { item_id, part, .. }
            | ResponseStreamEvent::ContentPartDone { item_id, part, .. } => match part {
                ResponseStreamContentPart::OutputText(text) => {
                    if let Some(chunk) = self.text_chunk(text.text) {
                        out.push(chunk);
                    }
                }
                ResponseStreamContentPart::Refusal(refusal) => {
                    if let Some(chunk) = self.text_chunk(refusal.refusal) {
                        out.push(chunk);
                    }
                }
                ResponseStreamContentPart::ReasoningText(reasoning) => {
                    if let Some(chunk) = self.thinking_chunk(item_id, reasoning.text) {
                        out.push(chunk);
                    }
                }
            },
            ResponseStreamEvent::OutputItemAdded { item, .. }
            | ResponseStreamEvent::OutputItemDone { item, .. } => {
                self.map_output_item(item, out);
            }
            ResponseStreamEvent::Error { error, .. } => {
                if let Some(chunk) = self.text_chunk(error.message) {
                    out.push(chunk);
                }
            }
            _ => {}
        }
    }

    pub fn finish(&mut self, out: &mut Vec<GeminiGenerateContentResponseBody>) {
        if !self.finished {
            self.finished = true;
            let _ = out;
        }
    }
}
