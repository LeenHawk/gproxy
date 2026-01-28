use std::collections::BTreeMap;

use gproxy_protocol::claude::count_tokens::types::Model as ClaudeModel;
use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamMessage, BetaStreamUsage, BetaThinkingBlockStream,
};
use gproxy_protocol::claude::create_message::types::{
    BetaServerToolName, BetaStopReason, BetaToolUseBlock, BetaToolUseBlockType, JsonObject,
    JsonValue,
};
use gproxy_protocol::gemini::count_tokens::types::{
    Content as GeminiContent, ContentRole as GeminiContentRole, FunctionCall as GeminiFunctionCall,
    Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    ServerTool,
    McpTool,
}

#[derive(Debug, Clone)]
struct ToolInfo {
    id: String,
    name: String,
    kind: ToolKind,
    server_name: Option<String>,
    arguments: String,
    emitted: bool,
}

#[derive(Debug, Clone)]
pub struct ClaudeToGeminiStreamState {
    response_id: String,
    model_version: String,
    stop_reason: Option<BetaStopReason>,
    usage: Option<BetaStreamUsage>,
    tool_blocks: BTreeMap<u32, ToolInfo>,
    finished: bool,
}

impl ClaudeToGeminiStreamState {
    pub fn new() -> Self {
        Self {
            response_id: "response".to_string(),
            model_version: "models/unknown".to_string(),
            stop_reason: None,
            usage: None,
            tool_blocks: BTreeMap::new(),
            finished: false,
        }
    }

    pub fn transform_event(&mut self, event: BetaStreamEvent) -> Vec<GenerateContentResponse> {
        let event = match event {
            BetaStreamEvent::Known(event) => event,
            BetaStreamEvent::Unknown(_) => return Vec::new(),
        };

        match event {
            BetaStreamEventKnown::MessageStart { message } => {
                self.update_from_message(&message);
                Vec::new()
            }
            BetaStreamEventKnown::ContentBlockStart {
                index,
                content_block,
            } => self.handle_block_start(index, content_block),
            BetaStreamEventKnown::ContentBlockDelta { index, delta } => {
                self.handle_block_delta(index, delta)
            }
            BetaStreamEventKnown::ContentBlockStop { index } => self.handle_block_stop(index),
            BetaStreamEventKnown::MessageDelta { delta, usage } => {
                self.stop_reason = delta.stop_reason;
                if usage.input_tokens.is_some() || usage.output_tokens.is_some() {
                    self.usage = Some(usage);
                }
                Vec::new()
            }
            BetaStreamEventKnown::MessageStop => self.finish_response(),
            BetaStreamEventKnown::Ping => Vec::new(),
            BetaStreamEventKnown::Error { .. } => Vec::new(),
        }
    }

    fn handle_block_start(
        &mut self,
        index: u32,
        content_block: BetaStreamContentBlock,
    ) -> Vec<GenerateContentResponse> {
        match content_block {
            BetaStreamContentBlock::Text(text) => self.emit_parts(vec![text_part(text.text)]),
            BetaStreamContentBlock::Thinking(thinking) => {
                self.emit_parts(vec![thinking_part(thinking)])
            }
            BetaStreamContentBlock::RedactedThinking(redacted) => {
                self.emit_parts(vec![redacted_thinking_part(redacted.data)])
            }
            BetaStreamContentBlock::ToolUse(tool) => {
                self.start_tool(index, tool, ToolKind::Function, None)
            }
            BetaStreamContentBlock::ServerToolUse(tool) => self.start_tool(
                index,
                BetaToolUseBlock {
                    id: tool.id,
                    input: tool.input,
                    name: server_tool_name(tool.name),
                    r#type: BetaToolUseBlockType::ToolUse,
                    caller: tool.caller,
                },
                ToolKind::ServerTool,
                None,
            ),
            BetaStreamContentBlock::McpToolUse(tool) => {
                let name = format!("mcp:{}:{}", tool.server_name, tool.name);
                self.start_tool(
                    index,
                    BetaToolUseBlock {
                        id: tool.id,
                        input: tool.input,
                        name,
                        r#type: BetaToolUseBlockType::ToolUse,
                        caller: None,
                    },
                    ToolKind::McpTool,
                    Some(tool.server_name),
                )
            }
            other => self.emit_serialized_block(other),
        }
    }

    fn handle_block_delta(
        &mut self,
        index: u32,
        delta: BetaStreamContentBlockDelta,
    ) -> Vec<GenerateContentResponse> {
        match delta {
            BetaStreamContentBlockDelta::TextDelta { text } => {
                self.emit_parts(vec![text_part(text)])
            }
            BetaStreamContentBlockDelta::ThinkingDelta { thinking } => {
                self.emit_parts(vec![thinking_delta_part(thinking)])
            }
            BetaStreamContentBlockDelta::InputJsonDelta { partial_json } => {
                self.append_tool_arguments(index, partial_json)
            }
            BetaStreamContentBlockDelta::SignatureDelta { signature } => {
                if signature.is_empty() {
                    Vec::new()
                } else {
                    self.emit_parts(vec![signature_part(signature)])
                }
            }
        }
    }

    fn handle_block_stop(&mut self, index: u32) -> Vec<GenerateContentResponse> {
        let info = match self.tool_blocks.get_mut(&index) {
            Some(info) => info,
            None => return Vec::new(),
        };

        if info.emitted {
            return Vec::new();
        }

        let args_value = if info.arguments.is_empty() {
            JsonValue::Object(serde_json::Map::new())
        } else {
            parse_json_value(&info.arguments)
        };

        info.emitted = true;
        let part = build_tool_part(info, args_value);
        self.emit_parts(vec![part])
    }

    fn start_tool(
        &mut self,
        index: u32,
        tool: BetaToolUseBlock,
        kind: ToolKind,
        server_name: Option<String>,
    ) -> Vec<GenerateContentResponse> {
        let args_value = json_object_to_value(&tool.input);
        let arguments = serde_json::to_string(&args_value).unwrap_or_default();
        let emitted = !arguments.is_empty();

        self.tool_blocks.insert(
            index,
            ToolInfo {
                id: tool.id.clone(),
                name: tool.name.clone(),
                kind,
                server_name,
                arguments,
                emitted,
            },
        );

        if emitted {
            let info = self.tool_blocks.get(&index).expect("tool info");
            let part = build_tool_part(info, args_value);
            self.emit_parts(vec![part])
        } else {
            Vec::new()
        }
    }

    fn append_tool_arguments(&mut self, index: u32, delta: String) -> Vec<GenerateContentResponse> {
        let info = match self.tool_blocks.get_mut(&index) {
            Some(info) => info,
            None => return Vec::new(),
        };

        info.arguments.push_str(&delta);
        info.emitted = true;
        let args_value = parse_json_value(&info.arguments);
        let part = build_tool_part(info, args_value);
        self.emit_parts(vec![part])
    }

    fn emit_parts(&self, parts: Vec<GeminiPart>) -> Vec<GenerateContentResponse> {
        let parts: Vec<GeminiPart> = parts.into_iter().filter(part_has_payload).collect();
        if parts.is_empty() {
            return Vec::new();
        }

        let candidate = Candidate {
            content: GeminiContent {
                parts,
                role: Some(GeminiContentRole::Model),
            },
            finish_reason: None,
            safety_ratings: None,
            citation_metadata: None,
            token_count: None,
            grounding_attributions: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
        };

        vec![GenerateContentResponse {
            candidates: vec![candidate],
            prompt_feedback: None,
            usage_metadata: None,
            model_version: Some(self.model_version.clone()),
            response_id: Some(self.response_id.clone()),
            model_status: None,
        }]
    }

    fn emit_serialized_block(&self, block: BetaStreamContentBlock) -> Vec<GenerateContentResponse> {
        let text = serde_json::to_string(&block).unwrap_or_default();
        if text.is_empty() {
            Vec::new()
        } else {
            self.emit_parts(vec![text_part(text)])
        }
    }

    fn update_from_message(&mut self, message: &BetaStreamMessage) {
        self.response_id = message.id.clone();
        self.model_version = map_model_version(&message.model);
    }

    fn finish_response(&mut self) -> Vec<GenerateContentResponse> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let finish_reason = self
            .stop_reason
            .map(map_stop_reason)
            .unwrap_or(FinishReason::Stop);

        let candidate = Candidate {
            content: GeminiContent {
                parts: Vec::new(),
                role: Some(GeminiContentRole::Model),
            },
            finish_reason: Some(finish_reason),
            safety_ratings: None,
            citation_metadata: None,
            token_count: None,
            grounding_attributions: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
        };

        vec![GenerateContentResponse {
            candidates: vec![candidate],
            prompt_feedback: None,
            usage_metadata: self.usage.as_ref().map(map_usage),
            model_version: Some(self.model_version.clone()),
            response_id: Some(self.response_id.clone()),
            model_status: None,
        }]
    }
}

impl Default for ClaudeToGeminiStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn text_part(text: String) -> GeminiPart {
    GeminiPart {
        text: Some(text),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn thinking_part(block: BetaThinkingBlockStream) -> GeminiPart {
    GeminiPart {
        text: Some(block.thinking),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: Some(true),
        thought_signature: block.signature,
        part_metadata: None,
        video_metadata: None,
    }
}

fn thinking_delta_part(thinking: String) -> GeminiPart {
    GeminiPart {
        text: Some(thinking),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: Some(true),
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn redacted_thinking_part(text: String) -> GeminiPart {
    GeminiPart {
        text: Some(text),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: Some(true),
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn signature_part(signature: String) -> GeminiPart {
    GeminiPart {
        text: None,
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: Some(true),
        thought_signature: Some(signature),
        part_metadata: None,
        video_metadata: None,
    }
}

fn build_tool_part(tool: &ToolInfo, args_value: JsonValue) -> GeminiPart {
    let args = match tool.kind {
        ToolKind::McpTool => {
            let mut map = serde_json::Map::new();
            if let Some(server_name) = &tool.server_name {
                map.insert(
                    "server_name".to_string(),
                    JsonValue::String(server_name.clone()),
                );
            }
            map.insert("input".to_string(), args_value);
            JsonValue::Object(map)
        }
        _ => args_value,
    };

    GeminiPart {
        text: None,
        inline_data: None,
        function_call: Some(GeminiFunctionCall {
            id: Some(tool.id.clone()),
            name: tool.name.clone(),
            args: Some(args),
        }),
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn part_has_payload(part: &GeminiPart) -> bool {
    part.text
        .as_ref()
        .map(|text| !text.is_empty())
        .unwrap_or(false)
        || part.function_call.is_some()
        || part.function_response.is_some()
        || part.inline_data.is_some()
        || part.file_data.is_some()
        || part.executable_code.is_some()
        || part.code_execution_result.is_some()
        || part.thought.is_some()
        || part.thought_signature.is_some()
        || part.part_metadata.is_some()
        || part.video_metadata.is_some()
}

fn json_object_to_value(value: &JsonObject) -> JsonValue {
    JsonValue::Object(value.clone().into_iter().collect())
}

fn parse_json_value(raw: &str) -> JsonValue {
    serde_json::from_str(raw).unwrap_or_else(|_| JsonValue::String(raw.to_string()))
}

fn server_tool_name(name: BetaServerToolName) -> String {
    match serde_json::to_value(name) {
        Ok(JsonValue::String(value)) => value,
        _ => "server_tool".to_string(),
    }
}

fn map_stop_reason(reason: BetaStopReason) -> FinishReason {
    match reason {
        BetaStopReason::EndTurn | BetaStopReason::StopSequence => FinishReason::Stop,
        BetaStopReason::MaxTokens => FinishReason::MaxTokens,
        BetaStopReason::ToolUse => FinishReason::Stop,
        BetaStopReason::Refusal => FinishReason::Safety,
        BetaStopReason::PauseTurn | BetaStopReason::ModelContextWindowExceeded => {
            FinishReason::Other
        }
    }
}

fn map_usage(usage: &BetaStreamUsage) -> UsageMetadata {
    let input_tokens = usage.input_tokens;
    let output_tokens = usage.output_tokens;
    let total = match (input_tokens, output_tokens) {
        (Some(input), Some(output)) => Some(input.saturating_add(output)),
        _ => None,
    };

    UsageMetadata {
        prompt_token_count: input_tokens,
        cached_content_token_count: usage.cache_read_input_tokens.filter(|count| *count > 0),
        candidates_token_count: output_tokens,
        tool_use_prompt_token_count: None,
        thoughts_token_count: None,
        total_token_count: total,
        prompt_tokens_details: None,
        cache_tokens_details: None,
        candidates_tokens_details: None,
        tool_use_prompt_tokens_details: None,
    }
}

fn map_model_version(model: &ClaudeModel) -> String {
    let model_id = match model {
        ClaudeModel::Custom(value) => value.clone(),
        ClaudeModel::Known(known) => match serde_json::to_value(known) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        },
    };

    if model_id.starts_with("models/") {
        model_id
    } else {
        format!("models/{}", model_id)
    }
}
