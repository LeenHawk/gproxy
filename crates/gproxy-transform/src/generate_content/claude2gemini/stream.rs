use std::collections::BTreeMap;

use gproxy_protocol::claude::count_tokens::types::Model as ClaudeModel;
use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamMessage, BetaStreamMessageDelta, BetaStreamUsage,
};
use gproxy_protocol::claude::create_message::types::{
    BetaMessageRole, BetaMessageType, BetaStopReason, BetaTextBlock, BetaTextBlockType,
    BetaToolUseBlock, BetaToolUseBlockType, JsonObject,
};
use gproxy_protocol::gemini::count_tokens::types::{
    FunctionCall as GeminiFunctionCall, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};

#[derive(Debug, Clone)]
struct ToolInfo {
    block_index: u32,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct GeminiToClaudeStreamState {
    id: String,
    model: ClaudeModel,
    message_started: bool,
    next_block_index: u32,
    text_block_index: Option<u32>,
    text_buffer: String,
    tool_blocks: BTreeMap<String, ToolInfo>,
    finished: bool,
}

impl GeminiToClaudeStreamState {
    pub fn new() -> Self {
        Self {
            id: "response".to_string(),
            model: ClaudeModel::Custom("unknown".to_string()),
            message_started: false,
            next_block_index: 0,
            text_block_index: None,
            text_buffer: String::new(),
            tool_blocks: BTreeMap::new(),
            finished: false,
        }
    }

    pub fn transform_response(
        &mut self,
        response: GenerateContentResponse,
    ) -> Vec<BetaStreamEvent> {
        if self.finished {
            return Vec::new();
        }

        self.update_from_response(&response);

        let mut events = self.ensure_message_start();

        if let Some(candidate) = response.candidates.first() {
            events.extend(self.handle_candidate(candidate));

            if let Some(finish_reason) = candidate.finish_reason
                && !self.finished
            {
                self.finished = true;
                events.extend(self.close_open_blocks());
                events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageDelta {
                    delta: BetaStreamMessageDelta {
                        stop_reason: Some(map_finish_reason(finish_reason)),
                        stop_sequence: None,
                    },
                    usage: map_usage(response.usage_metadata),
                }));
                events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageStop));
            }
        }

        events
    }

    fn handle_candidate(&mut self, candidate: &Candidate) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();
        for part in &candidate.content.parts {
            events.extend(self.handle_part(part));
        }
        events
    }

    fn handle_part(&mut self, part: &GeminiPart) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();

        if let Some(text) = &part.text {
            events.extend(self.emit_text(text.clone()));
        }

        if let Some(function_call) = &part.function_call {
            events.extend(self.handle_function_call(function_call));
        }

        if let Some(function_response) = &part.function_response
            && let Ok(text) = serde_json::to_string(function_response)
        {
            events.extend(self.emit_text(text));
        }

        if let Some(code) = &part.executable_code
            && let Ok(text) = serde_json::to_string(code)
        {
            events.extend(self.emit_text(text));
        }

        if let Some(result) = &part.code_execution_result
            && let Ok(text) = serde_json::to_string(result)
        {
            events.extend(self.emit_text(text));
        }

        if part.inline_data.is_some() {
            events.extend(self.emit_text("[inline_data]".to_string()));
        }

        if let Some(file_data) = &part.file_data {
            events.extend(self.emit_text(format!("[file:{}]", file_data.file_uri)));
        }

        events
    }

    fn handle_function_call(&mut self, call: &GeminiFunctionCall) -> Vec<BetaStreamEvent> {
        let id = call.id.clone().unwrap_or_else(|| call.name.clone());
        let mut events = self.ensure_tool(id.clone(), call.name.clone());
        let arguments = call
            .args
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .unwrap_or_default();

        if !arguments.is_empty() {
            events.extend(self.append_tool_arguments(&id, arguments));
        }

        events
    }

    fn emit_text(&mut self, text: String) -> Vec<BetaStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let delta = self.apply_text_delta(&text);
        if delta.is_empty() {
            return Vec::new();
        }

        let mut events = self.ensure_message_start();
        let index = match self.text_block_index {
            Some(index) => index,
            None => {
                let index = self.next_block_index;
                self.next_block_index += 1;
                self.text_block_index = Some(index);
                events.push(BetaStreamEvent::Known(
                    BetaStreamEventKnown::ContentBlockStart {
                        index,
                        content_block: BetaStreamContentBlock::Text(BetaTextBlock {
                            citations: None,
                            text: String::new(),
                            r#type: BetaTextBlockType::Text,
                        }),
                    },
                ));
                index
            }
        };

        events.push(BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockDelta {
                index,
                delta: BetaStreamContentBlockDelta::TextDelta { text: delta },
            },
        ));
        events
    }

    fn apply_text_delta(&mut self, incoming: &str) -> String {
        if self.text_buffer.is_empty() {
            self.text_buffer = incoming.to_string();
            return incoming.to_string();
        }

        if incoming.starts_with(&self.text_buffer) {
            let delta = incoming[self.text_buffer.len()..].to_string();
            self.text_buffer = incoming.to_string();
            return delta;
        }

        self.text_buffer.push_str(incoming);
        incoming.to_string()
    }

    fn ensure_tool(&mut self, id: String, name: String) -> Vec<BetaStreamEvent> {
        if self.tool_blocks.contains_key(&id) {
            return Vec::new();
        }

        let block_index = self.next_block_index;
        self.next_block_index += 1;
        self.tool_blocks.insert(
            id.clone(),
            ToolInfo {
                block_index,
                arguments: String::new(),
            },
        );

        vec![BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockStart {
                index: block_index,
                content_block: BetaStreamContentBlock::ToolUse(BetaToolUseBlock {
                    id,
                    input: JsonObject::new(),
                    name,
                    r#type: BetaToolUseBlockType::ToolUse,
                    caller: None,
                }),
            },
        )]
    }

    fn append_tool_arguments(&mut self, id: &str, arguments: String) -> Vec<BetaStreamEvent> {
        let info = match self.tool_blocks.get_mut(id) {
            Some(info) => info,
            None => return Vec::new(),
        };

        let delta = if arguments.starts_with(&info.arguments) {
            arguments[info.arguments.len()..].to_string()
        } else {
            arguments.clone()
        };

        if delta.is_empty() {
            return Vec::new();
        }

        info.arguments = arguments;
        vec![BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockDelta {
                index: info.block_index,
                delta: BetaStreamContentBlockDelta::InputJsonDelta {
                    partial_json: delta,
                },
            },
        )]
    }

    fn ensure_message_start(&mut self) -> Vec<BetaStreamEvent> {
        if self.message_started {
            return Vec::new();
        }
        self.message_started = true;
        vec![BetaStreamEvent::Known(BetaStreamEventKnown::MessageStart {
            message: BetaStreamMessage {
                id: self.id.clone(),
                container: None,
                content: Vec::new(),
                context_management: None,
                model: self.model.clone(),
                role: BetaMessageRole::Assistant,
                stop_reason: None,
                stop_sequence: None,
                r#type: BetaMessageType::Message,
                usage: empty_usage(),
            },
        })]
    }

    fn close_open_blocks(&mut self) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();
        if let Some(index) = self.text_block_index.take() {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop { index },
            ));
        }
        let tool_blocks = std::mem::take(&mut self.tool_blocks);
        for (_, info) in tool_blocks {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop {
                    index: info.block_index,
                },
            ));
        }
        events
    }

    fn update_from_response(&mut self, response: &GenerateContentResponse) {
        if let Some(id) = response.response_id.clone() {
            self.id = id;
        }

        let model_id = response
            .model_version
            .clone()
            .or_else(|| {
                response
                    .model_status
                    .as_ref()
                    .map(|status| format!("{:?}", status.model_stage))
            })
            .unwrap_or_else(|| "unknown".to_string());

        let model_id = model_id.strip_prefix("models/").unwrap_or(&model_id);
        self.model = ClaudeModel::Custom(model_id.to_string());
    }
}

impl Default for GeminiToClaudeStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn empty_usage() -> BetaStreamUsage {
    BetaStreamUsage {
        input_tokens: None,
        output_tokens: None,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        server_tool_use: None,
    }
}

fn map_usage(usage: Option<UsageMetadata>) -> BetaStreamUsage {
    let input_tokens = usage.as_ref().and_then(|usage| usage.prompt_token_count);
    let output_tokens = usage
        .as_ref()
        .and_then(|usage| usage.candidates_token_count);

    BetaStreamUsage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        server_tool_use: None,
    }
}

fn map_finish_reason(reason: FinishReason) -> BetaStopReason {
    match reason {
        FinishReason::Stop => BetaStopReason::EndTurn,
        FinishReason::MaxTokens => BetaStopReason::MaxTokens,
        FinishReason::MalformedFunctionCall
        | FinishReason::UnexpectedToolCall
        | FinishReason::TooManyToolCalls => BetaStopReason::ToolUse,
        FinishReason::Safety
        | FinishReason::Blocklist
        | FinishReason::ProhibitedContent
        | FinishReason::Spii
        | FinishReason::ImageSafety
        | FinishReason::ImageProhibitedContent
        | FinishReason::ImageRecitation
        | FinishReason::NoImage
        | FinishReason::Recitation => BetaStopReason::Refusal,
        _ => BetaStopReason::EndTurn,
    }
}
