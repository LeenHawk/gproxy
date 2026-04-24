use std::collections::{BTreeMap, BTreeSet};

use crate::claude::create_message::stream::ClaudeStreamEvent;
use crate::claude::create_message::types::{BetaServiceTier, BetaStopReason};
use crate::openai::create_response::response::ResponseBody as OpenAiCreateResponseBody;
use crate::openai::create_response::stream::{ResponseStreamContentPart, ResponseStreamEvent};
use crate::openai::create_response::types::{
    ResponseIncompleteReason, ResponseOutputItem, ResponseServiceTier, ResponseUsage,
};
use crate::transform::claude::stream_generate_content::utils::{
    input_json_delta_event, message_delta_event, message_start_event, message_stop_event,
    push_text_block, push_thinking_block, start_text_block_event, start_thinking_block_event,
    start_tool_use_block_event, stop_block_event, stream_error_event, text_delta_event,
    thinking_delta_event,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamState {
    Init,
    Running,
    Finished,
}

#[derive(Debug, Clone)]
pub struct OpenAiResponseToClaudeStream {
    state: StreamState,
    next_block_index: u64,
    message_id: String,
    model: String,
    service_tier: BetaServiceTier,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    stop_reason: Option<BetaStopReason>,
    has_tool_use: bool,
    has_refusal: bool,
    open_text_blocks: BTreeMap<(String, u64, u64), u64>,
    open_thinking_blocks: BTreeMap<(String, u64, u64), u64>,
    open_summary_blocks: BTreeMap<(String, u64, u64), u64>,
    open_tool_blocks: BTreeMap<String, u64>,
    completed_text_blocks: BTreeSet<(String, u64, u64)>,
    completed_thinking_blocks: BTreeSet<(String, u64, u64)>,
    completed_summary_blocks: BTreeSet<(String, u64, u64)>,
    streamed_message_items: BTreeSet<String>,
    streamed_tool_args: BTreeSet<String>,
}

impl Default for OpenAiResponseToClaudeStream {
    fn default() -> Self {
        Self {
            state: StreamState::Init,
            next_block_index: 0,
            message_id: String::new(),
            model: String::new(),
            service_tier: BetaServiceTier::Standard,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 0,
            stop_reason: None,
            has_tool_use: false,
            has_refusal: false,
            open_text_blocks: BTreeMap::new(),
            open_thinking_blocks: BTreeMap::new(),
            open_summary_blocks: BTreeMap::new(),
            open_tool_blocks: BTreeMap::new(),
            completed_text_blocks: BTreeSet::new(),
            completed_thinking_blocks: BTreeSet::new(),
            completed_summary_blocks: BTreeSet::new(),
            streamed_message_items: BTreeSet::new(),
            streamed_tool_args: BTreeSet::new(),
        }
    }
}

impl OpenAiResponseToClaudeStream {
    fn web_search_item_id(
        id: Option<String>,
        action: &crate::openai::count_tokens::types::ResponseFunctionWebSearchAction,
    ) -> String {
        id.unwrap_or_else(|| match action {
            crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::Search {
                query,
                queries,
                ..
            } => query
                .clone()
                .or_else(|| queries.as_ref().and_then(|items| items.first().cloned()))
                .unwrap_or_else(|| "web_search".to_string()),
            crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::OpenPage {
                url,
            } => url
                .clone()
                .unwrap_or_else(|| "web_search_open_page".to_string()),
            crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::FindInPage {
                pattern,
                url,
            } => format!("web_search_find_in_page:{pattern}:{url}"),
        })
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.state, StreamState::Finished)
    }

    fn apply_usage(&mut self, usage: &ResponseUsage) {
        let cached_tokens = usage.input_tokens_details.cached_tokens;
        let total_input_tokens = if usage.total_tokens >= usage.output_tokens {
            usage.total_tokens.saturating_sub(usage.output_tokens)
        } else {
            usage.input_tokens
        };
        self.input_tokens = total_input_tokens.saturating_sub(cached_tokens);
        self.cached_input_tokens = cached_tokens;
        self.output_tokens = usage.output_tokens;
    }

    fn next_block(&mut self) -> u64 {
        let index = self.next_block_index;
        self.next_block_index = self.next_block_index.saturating_add(1);
        index
    }

    fn ensure_running(&mut self, out: &mut Vec<ClaudeStreamEvent>) {
        if matches!(self.state, StreamState::Init) {
            out.push(message_start_event(
                self.message_id.clone(),
                self.model.clone(),
                self.service_tier.clone(),
                self.input_tokens,
                self.cached_input_tokens,
            ));
            self.state = StreamState::Running;
        }
    }

    fn apply_response_state(
        &mut self,
        response: &OpenAiCreateResponseBody,
        out: &mut Vec<ClaudeStreamEvent>,
    ) {
        self.message_id = response.id.clone();
        self.model = response.model.clone();
        self.service_tier = match response.service_tier {
            Some(ResponseServiceTier::Priority) => BetaServiceTier::Priority,
            _ => BetaServiceTier::Standard,
        };
        if let Some(usage) = response.usage.as_ref() {
            self.apply_usage(usage);
        }
        self.ensure_running(out);
    }

    fn emit_text_block(&mut self, out: &mut Vec<ClaudeStreamEvent>, text: String) {
        self.ensure_running(out);
        let _ = push_text_block(out, &mut self.next_block_index, text);
    }

    fn emit_thinking_block(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        signature: String,
        thinking: String,
    ) {
        self.ensure_running(out);
        let _ = push_thinking_block(out, &mut self.next_block_index, signature, thinking);
    }

    fn ensure_tool_block(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        item_id: &str,
        name: &str,
    ) -> u64 {
        self.has_tool_use = true;
        if let Some(index) = self.open_tool_blocks.get(item_id) {
            *index
        } else {
            let index = self.next_block();
            out.push(start_tool_use_block_event(
                index,
                item_id.to_string(),
                name.to_string(),
            ));
            self.open_tool_blocks.insert(item_id.to_string(), index);
            index
        }
    }

    fn close_tool_block(&mut self, out: &mut Vec<ClaudeStreamEvent>, item_id: &str) {
        if let Some(index) = self.open_tool_blocks.remove(item_id) {
            out.push(stop_block_event(index));
        }
    }

    fn finish_text_block(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        key: (String, u64, u64),
        text: String,
    ) {
        if !self.completed_text_blocks.insert(key.clone()) {
            return;
        }
        if let Some(index) = self.open_text_blocks.remove(&key) {
            out.push(stop_block_event(index));
            return;
        }
        let index = self.next_block();
        out.push(start_text_block_event(index));
        if !text.is_empty() {
            out.push(text_delta_event(index, text));
        }
        out.push(stop_block_event(index));
    }

    fn finish_thinking_block(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        key: (String, u64, u64),
        signature: String,
        text: String,
    ) {
        if !self.completed_thinking_blocks.insert(key.clone()) {
            return;
        }
        if let Some(index) = self.open_thinking_blocks.remove(&key) {
            out.push(stop_block_event(index));
            return;
        }
        let index = self.next_block();
        out.push(start_thinking_block_event(index, signature));
        if !text.is_empty() {
            out.push(thinking_delta_event(index, text));
        }
        out.push(stop_block_event(index));
    }

    fn finish_summary_block(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        key: (String, u64, u64),
        signature: String,
        text: String,
    ) {
        if !self.completed_summary_blocks.insert(key.clone()) {
            return;
        }
        if let Some(index) = self.open_summary_blocks.remove(&key) {
            out.push(stop_block_event(index));
            return;
        }
        let index = self.next_block();
        out.push(start_thinking_block_event(index, signature));
        if !text.is_empty() {
            out.push(thinking_delta_event(index, text));
        }
        out.push(stop_block_event(index));
    }

    fn map_output_item(
        &mut self,
        out: &mut Vec<ClaudeStreamEvent>,
        item: ResponseOutputItem,
        is_done: bool,
    ) {
        match item {
            ResponseOutputItem::Message(message) => {
                if !is_done {
                    self.streamed_message_items.insert(message.id.clone());
                }
                for part in message.content {
                    match part {
                        crate::openai::count_tokens::types::ResponseOutputContent::Text(text) => {
                            self.emit_text_block(out, text.text);
                        }
                        crate::openai::count_tokens::types::ResponseOutputContent::Refusal(
                            refusal,
                        ) => {
                            self.has_refusal = true;
                            self.emit_text_block(out, refusal.refusal);
                        }
                    }
                }
            }
            ResponseOutputItem::FunctionToolCall(call) => {
                let item_id = call.id.unwrap_or(call.call_id);
                let block_index = self.ensure_tool_block(out, &item_id, &call.name);
                if !call.arguments.is_empty() {
                    out.push(input_json_delta_event(block_index, call.arguments));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::CustomToolCall(call) => {
                let item_id = call.id.unwrap_or(call.call_id);
                let block_index = self.ensure_tool_block(out, &item_id, &call.name);
                if !call.input.is_empty() {
                    out.push(input_json_delta_event(block_index, call.input));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::McpCall(call) => {
                let item_id = call.id.clone();
                let block_index = self.ensure_tool_block(out, &item_id, &call.name);
                if !call.arguments.is_empty() {
                    out.push(input_json_delta_event(block_index, call.arguments));
                }
                if let Some(output) = call.output {
                    self.emit_text_block(out, format!("mcp_output({item_id}): {output}"));
                }
                if let Some(error) = call.error {
                    self.emit_text_block(out, format!("mcp_error({item_id}): {error}"));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::McpListTools(item) => {
                let item_id = item.id;
                let block_index = self.ensure_tool_block(out, &item_id, "mcp_list_tools");
                if let Ok(tools_json) = serde_json::to_string(&item.tools)
                    && !tools_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, tools_json));
                }
                if let Some(error) = item.error {
                    self.emit_text_block(out, format!("mcp_list_tools_error({item_id}): {error}"));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::McpApprovalRequest(item) => {
                let item_id = item.id;
                let block_index = self.ensure_tool_block(out, &item_id, &item.name);
                if !item.arguments.is_empty() {
                    out.push(input_json_delta_event(block_index, item.arguments));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::McpApprovalResponse(item) => {
                self.emit_text_block(
                    out,
                    format!(
                        "mcp_approval_response({}): approve={}{}",
                        item.approval_request_id,
                        item.approve,
                        item.reason
                            .map(|reason| format!(", reason={reason}"))
                            .unwrap_or_default()
                    ),
                );
            }
            ResponseOutputItem::FileSearchToolCall(call) => {
                let item_id = call.id;
                let block_index = self.ensure_tool_block(out, &item_id, "file_search");
                if let Ok(queries_json) = serde_json::to_string(&call.queries)
                    && !queries_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, queries_json));
                }
                if let Some(results) = call.results
                    && let Ok(results_json) = serde_json::to_string(&results)
                    && !results_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("file_search_results({item_id}): {results_json}"),
                    );
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::FunctionWebSearch(call) => {
                let item_id = Self::web_search_item_id(call.id, &call.action);
                let block_index = self.ensure_tool_block(out, &item_id, "web_search");
                if let Ok(action_json) = serde_json::to_string(&call.action)
                    && !action_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, action_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::CodeInterpreterToolCall(call) => {
                let item_id = call.id;
                let block_index = self.ensure_tool_block(out, &item_id, "code_interpreter");
                if !call.code.is_empty() {
                    out.push(input_json_delta_event(block_index, call.code));
                }
                if let Some(outputs) = call.outputs
                    && let Ok(outputs_json) = serde_json::to_string(&outputs)
                    && !outputs_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("code_interpreter_outputs({item_id}): {outputs_json}"),
                    );
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::ShellCall(call) => {
                let item_id = call.id.unwrap_or(call.call_id);
                let block_index = self.ensure_tool_block(out, &item_id, "shell_call");
                if let Ok(action_json) = serde_json::to_string(&call.action)
                    && !action_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, action_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::ShellCallOutput(call) => {
                if let Ok(output_json) = serde_json::to_string(&call.output)
                    && !output_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("shell_call_output({}): {output_json}", call.call_id),
                    );
                }
            }
            ResponseOutputItem::LocalShellCall(call) => {
                let item_id = call.id;
                let block_index = self.ensure_tool_block(out, &item_id, "local_shell_call");
                if let Ok(action_json) = serde_json::to_string(&call.action)
                    && !action_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, action_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::LocalShellCallOutput(call) => {
                if !call.output.is_empty() {
                    self.emit_text_block(
                        out,
                        format!("local_shell_output({}): {}", call.id, call.output),
                    );
                }
            }
            ResponseOutputItem::ApplyPatchCall(call) => {
                let item_id = call.id.unwrap_or(call.call_id);
                let block_index = self.ensure_tool_block(out, &item_id, "apply_patch");
                if let Ok(operation_json) = serde_json::to_string(&call.operation)
                    && !operation_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, operation_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::ApplyPatchCallOutput(call) => {
                let text = if let Some(output) = call.output {
                    format!("apply_patch_output({}): {}", call.call_id, output)
                } else {
                    format!("apply_patch_output({})", call.call_id)
                };
                self.emit_text_block(out, text);
            }
            ResponseOutputItem::FunctionCallOutput(call) => {
                if let Ok(output_json) = serde_json::to_string(&call.output)
                    && !output_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("function_call_output({}): {output_json}", call.call_id),
                    );
                }
            }
            ResponseOutputItem::CustomToolCallOutput(call) => {
                if let Ok(output_json) = serde_json::to_string(&call.output)
                    && !output_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("custom_tool_call_output({}): {output_json}", call.call_id),
                    );
                }
            }
            ResponseOutputItem::ComputerToolCall(call) => {
                let item_id = call.id;
                let block_index = self.ensure_tool_block(out, &item_id, "computer_call");
                if let Ok(action_json) = serde_json::to_string(&call.action)
                    && !action_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, action_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::ComputerCallOutput(call) => {
                if let Ok(output_json) = serde_json::to_string(&call.output)
                    && !output_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("computer_call_output({}): {output_json}", call.call_id),
                    );
                }
            }
            ResponseOutputItem::ToolSearchCall(call) => {
                let item_id = call.id;
                let block_index = self.ensure_tool_block(out, &item_id, "tool_search");
                if let Ok(arguments_json) = serde_json::to_string(&call.arguments)
                    && !arguments_json.is_empty()
                {
                    out.push(input_json_delta_event(block_index, arguments_json));
                }
                if is_done {
                    self.close_tool_block(out, &item_id);
                }
            }
            ResponseOutputItem::ToolSearchOutput(call) => {
                if let Ok(tools_json) = serde_json::to_string(&call.tools)
                    && !tools_json.is_empty()
                {
                    self.emit_text_block(
                        out,
                        format!("tool_search_output({}): {tools_json}", call.call_id),
                    );
                }
            }
            ResponseOutputItem::ReasoningItem(item) => {
                if let Some(signature) = item.id.filter(|id| !id.is_empty()) {
                    for summary in item.summary {
                        self.emit_thinking_block(out, signature.clone(), summary.text);
                    }
                    if let Some(content) = item.content {
                        for entry in content {
                            self.emit_thinking_block(out, signature.clone(), entry.text);
                        }
                    }
                    if let Some(encrypted_content) = item.encrypted_content
                        && !encrypted_content.is_empty()
                    {
                        self.emit_thinking_block(out, signature, encrypted_content);
                    }
                }
            }
            ResponseOutputItem::CompactionItem(item) => {
                self.emit_text_block(out, format!("compaction: {}", item.encrypted_content));
            }
            ResponseOutputItem::ImageGenerationCall(item) => {
                if let Some(result) = item.result.filter(|s| !s.is_empty()) {
                    self.emit_text_block(out, result);
                }
            }
            ResponseOutputItem::ItemReference(item) => {
                self.emit_text_block(out, format!("item_reference: {}", item.id));
            }
        }
    }

    pub fn on_stream_event(
        &mut self,
        stream_event: ResponseStreamEvent,
        out: &mut Vec<ClaudeStreamEvent>,
    ) {
        if self.is_finished() {
            return;
        }

        match stream_event {
            ResponseStreamEvent::Created { response, .. }
            | ResponseStreamEvent::Queued { response, .. }
            | ResponseStreamEvent::InProgress { response, .. } => {
                self.apply_response_state(&response, out);
            }
            ResponseStreamEvent::Completed { response, .. } => {
                self.apply_response_state(&response, out);
                self.stop_reason = match response
                    .incomplete_details
                    .as_ref()
                    .and_then(|details| details.reason.as_ref())
                {
                    Some(ResponseIncompleteReason::MaxOutputTokens) => {
                        Some(BetaStopReason::MaxTokens)
                    }
                    Some(ResponseIncompleteReason::ContentFilter) => Some(BetaStopReason::Refusal),
                    None => None,
                };
            }
            ResponseStreamEvent::Incomplete { response, .. } => {
                self.apply_response_state(&response, out);
                self.stop_reason = Some(
                    match response
                        .incomplete_details
                        .as_ref()
                        .and_then(|details| details.reason.as_ref())
                    {
                        Some(ResponseIncompleteReason::MaxOutputTokens) => {
                            BetaStopReason::MaxTokens
                        }
                        Some(ResponseIncompleteReason::ContentFilter) => BetaStopReason::Refusal,
                        None => BetaStopReason::EndTurn,
                    },
                );
            }
            ResponseStreamEvent::Failed { response, .. } => {
                self.apply_response_state(&response, out);
                if let Some(error) = response.error {
                    self.has_refusal = true;
                    out.push(stream_error_event(error.message));
                }
                self.stop_reason = Some(BetaStopReason::Refusal);
            }
            ResponseStreamEvent::AudioDelta { delta, .. } => {
                if !delta.is_empty() {
                    self.emit_text_block(out, format!("audio_delta: {delta}"));
                }
            }
            ResponseStreamEvent::AudioDone { .. } => {}
            ResponseStreamEvent::AudioTranscriptDelta { delta, .. } => {
                if !delta.is_empty() {
                    self.emit_text_block(out, delta);
                }
            }
            ResponseStreamEvent::AudioTranscriptDone { .. } => {}
            ResponseStreamEvent::CodeInterpreterCallInProgress { item_id, .. }
            | ResponseStreamEvent::CodeInterpreterCallInterpreting { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "code_interpreter");
            }
            ResponseStreamEvent::CodeInterpreterCallCodeDelta { delta, item_id, .. } => {
                let block_index = self.ensure_tool_block(out, &item_id, "code_interpreter");
                if !delta.is_empty() {
                    out.push(input_json_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::CodeInterpreterCallCodeDone { code, item_id, .. } => {
                let block_index = self.ensure_tool_block(out, &item_id, "code_interpreter");
                if !code.is_empty() {
                    out.push(input_json_delta_event(block_index, code));
                }
            }
            ResponseStreamEvent::CodeInterpreterCallCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::OutputItemAdded { item, .. } => {
                self.map_output_item(out, item, false);
            }
            ResponseStreamEvent::OutputItemDone { item, .. } => match item {
                ResponseOutputItem::Message(message)
                    if self.streamed_message_items.contains(&message.id) => {}
                ResponseOutputItem::FunctionToolCall(call) => {
                    let item_id = call
                        .id
                        .as_deref()
                        .unwrap_or(call.call_id.as_str())
                        .to_string();
                    if self.streamed_tool_args.contains(&item_id) {
                        self.close_tool_block(out, &item_id);
                    } else {
                        self.map_output_item(out, ResponseOutputItem::FunctionToolCall(call), true);
                    }
                }
                ResponseOutputItem::CustomToolCall(call) => {
                    let item_id = call
                        .id
                        .as_deref()
                        .unwrap_or(call.call_id.as_str())
                        .to_string();
                    if self.streamed_tool_args.contains(&item_id) {
                        self.close_tool_block(out, &item_id);
                    } else {
                        self.map_output_item(out, ResponseOutputItem::CustomToolCall(call), true);
                    }
                }
                item => self.map_output_item(out, item, true),
            },
            ResponseStreamEvent::ContentPartAdded {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => match part {
                ResponseStreamContentPart::OutputText(text) => {
                    self.streamed_message_items.insert(item_id.clone());
                    self.ensure_running(out);
                    let key = (item_id.clone(), output_index, content_index);
                    let block_index = if let Some(index) = self.open_text_blocks.get(&key) {
                        *index
                    } else {
                        let index = self.next_block();
                        out.push(start_text_block_event(index));
                        self.open_text_blocks.insert(key, index);
                        index
                    };
                    if !text.text.is_empty() {
                        out.push(text_delta_event(block_index, text.text));
                    }
                }
                ResponseStreamContentPart::Refusal(refusal) => {
                    self.has_refusal = true;
                    self.streamed_message_items.insert(item_id.clone());
                    self.ensure_running(out);
                    let key = (item_id.clone(), output_index, content_index);
                    let block_index = if let Some(index) = self.open_text_blocks.get(&key) {
                        *index
                    } else {
                        let index = self.next_block();
                        out.push(start_text_block_event(index));
                        self.open_text_blocks.insert(key, index);
                        index
                    };
                    if !refusal.refusal.is_empty() {
                        out.push(text_delta_event(block_index, refusal.refusal));
                    }
                }
                ResponseStreamContentPart::ReasoningText(reasoning) => {
                    self.streamed_message_items.insert(item_id.clone());
                    self.ensure_running(out);
                    let key = (item_id.clone(), output_index, content_index);
                    let block_index = if let Some(index) = self.open_thinking_blocks.get(&key) {
                        *index
                    } else {
                        let index = self.next_block();
                        out.push(start_thinking_block_event(
                            index,
                            format!("{item_id}_{output_index}_{content_index}"),
                        ));
                        self.open_thinking_blocks.insert(key, index);
                        index
                    };
                    if !reasoning.text.is_empty() {
                        out.push(thinking_delta_event(block_index, reasoning.text));
                    }
                }
            },
            ResponseStreamEvent::ContentPartDone {
                content_index,
                item_id,
                output_index,
                part,
                ..
            } => match part {
                ResponseStreamContentPart::OutputText(text) => {
                    self.finish_text_block(out, (item_id, output_index, content_index), text.text);
                }
                ResponseStreamContentPart::Refusal(refusal) => {
                    self.has_refusal = true;
                    self.finish_text_block(
                        out,
                        (item_id, output_index, content_index),
                        refusal.refusal,
                    );
                }
                ResponseStreamContentPart::ReasoningText(reasoning) => {
                    let signature = format!("{item_id}_{output_index}_{content_index}");
                    self.finish_thinking_block(
                        out,
                        (item_id, output_index, content_index),
                        signature,
                        reasoning.text,
                    );
                }
            },
            ResponseStreamEvent::OutputTextAnnotationAdded {
                annotation,
                annotation_index,
                content_index,
                item_id,
                output_index,
                ..
            } => {
                let annotation_text = annotation.to_string();
                if !annotation_text.is_empty() {
                    self.emit_text_block(
                        out,
                        format!(
                            "annotation({item_id}:{output_index}:{content_index}:{annotation_index}): {annotation_text}"
                        ),
                    );
                }
            }
            ResponseStreamEvent::OutputTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.streamed_message_items.insert(item_id.clone());
                self.ensure_running(out);
                let key = (item_id.clone(), output_index, content_index);
                let block_index = if let Some(index) = self.open_text_blocks.get(&key) {
                    *index
                } else {
                    let index = self.next_block();
                    out.push(start_text_block_event(index));
                    self.open_text_blocks.insert(key, index);
                    index
                };
                if !delta.is_empty() {
                    out.push(text_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::OutputTextDone {
                content_index,
                item_id,
                output_index,
                text,
                ..
            } => {
                self.finish_text_block(out, (item_id, output_index, content_index), text);
            }
            ResponseStreamEvent::RefusalDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.has_refusal = true;
                self.streamed_message_items.insert(item_id.clone());
                let key = (item_id.clone(), output_index, content_index);
                let block_index = if let Some(index) = self.open_text_blocks.get(&key) {
                    *index
                } else {
                    let index = self.next_block();
                    out.push(start_text_block_event(index));
                    self.open_text_blocks.insert(key, index);
                    index
                };
                if !delta.is_empty() {
                    out.push(text_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::RefusalDone {
                content_index,
                item_id,
                output_index,
                refusal,
                ..
            } => {
                self.has_refusal = true;
                self.finish_text_block(out, (item_id, output_index, content_index), refusal);
            }
            ResponseStreamEvent::ReasoningTextDelta {
                content_index,
                delta,
                item_id,
                output_index,
                ..
            } => {
                self.streamed_message_items.insert(item_id.clone());
                let key = (item_id.clone(), output_index, content_index);
                let block_index = if let Some(index) = self.open_thinking_blocks.get(&key) {
                    *index
                } else {
                    let index = self.next_block();
                    out.push(start_thinking_block_event(
                        index,
                        format!("{item_id}_{output_index}_{content_index}"),
                    ));
                    self.open_thinking_blocks.insert(key, index);
                    index
                };
                if !delta.is_empty() {
                    out.push(thinking_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::ReasoningTextDone {
                content_index,
                item_id,
                output_index,
                text,
                ..
            } => {
                let signature = format!("{item_id}_{output_index}_{content_index}");
                self.finish_thinking_block(
                    out,
                    (item_id, output_index, content_index),
                    signature,
                    text,
                );
            }
            ResponseStreamEvent::ReasoningSummaryPartAdded {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                self.streamed_message_items.insert(item_id.clone());
                let key = (item_id.clone(), output_index, summary_index);
                let block_index = if let Some(index) = self.open_summary_blocks.get(&key) {
                    *index
                } else {
                    let index = self.next_block();
                    out.push(start_thinking_block_event(
                        index,
                        format!("{item_id}_{output_index}_summary_{summary_index}"),
                    ));
                    self.open_summary_blocks.insert(key, index);
                    index
                };
                if !part.text.is_empty() {
                    out.push(thinking_delta_event(block_index, part.text));
                }
            }
            ResponseStreamEvent::ReasoningSummaryPartDone {
                item_id,
                output_index,
                part,
                summary_index,
                ..
            } => {
                let signature = format!("{item_id}_{output_index}_summary_{summary_index}");
                self.finish_summary_block(
                    out,
                    (item_id, output_index, summary_index),
                    signature,
                    part.text,
                );
            }
            ResponseStreamEvent::ReasoningSummaryTextDelta {
                delta,
                item_id,
                output_index,
                summary_index,
                ..
            } => {
                self.streamed_message_items.insert(item_id.clone());
                let key = (item_id.clone(), output_index, summary_index);
                let block_index = if let Some(index) = self.open_summary_blocks.get(&key) {
                    *index
                } else {
                    let index = self.next_block();
                    out.push(start_thinking_block_event(
                        index,
                        format!("{item_id}_{output_index}_summary_{summary_index}"),
                    ));
                    self.open_summary_blocks.insert(key, index);
                    index
                };
                if !delta.is_empty() {
                    out.push(thinking_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::ReasoningSummaryTextDone {
                item_id,
                output_index,
                summary_index,
                text,
                ..
            } => {
                let signature = format!("{item_id}_{output_index}_summary_{summary_index}");
                self.finish_summary_block(
                    out,
                    (item_id, output_index, summary_index),
                    signature,
                    text,
                );
            }
            ResponseStreamEvent::FunctionCallArgumentsDelta { delta, item_id, .. } => {
                let block_index = self.ensure_tool_block(out, &item_id, "function");
                self.streamed_tool_args.insert(item_id.clone());
                if !delta.is_empty() {
                    out.push(input_json_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::FunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                ..
            } => {
                if !self.streamed_tool_args.contains(&item_id) {
                    let block_index = self.ensure_tool_block(
                        out,
                        &item_id,
                        name.as_deref().unwrap_or("function"),
                    );
                    if !arguments.is_empty() {
                        out.push(input_json_delta_event(block_index, arguments));
                    }
                }
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::FileSearchCallInProgress { item_id, .. }
            | ResponseStreamEvent::FileSearchCallSearching { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "file_search");
            }
            ResponseStreamEvent::FileSearchCallCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::WebSearchCallInProgress { item_id, .. }
            | ResponseStreamEvent::WebSearchCallSearching { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "web_search");
            }
            ResponseStreamEvent::WebSearchCallCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::ImageGenerationCallInProgress { item_id, .. }
            | ResponseStreamEvent::ImageGenerationCallGenerating { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "image_generation");
            }
            ResponseStreamEvent::ImageGenerationCallPartialImage {
                item_id,
                partial_image_b64,
                partial_image_index,
                ..
            } => {
                self.ensure_tool_block(out, &item_id, "image_generation");
                if !partial_image_b64.is_empty() {
                    self.emit_text_block(
                        out,
                        format!(
                            "image_partial({item_id}:{partial_image_index}): {partial_image_b64}"
                        ),
                    );
                }
            }
            ResponseStreamEvent::ImageGenerationCallCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::CustomToolCallInputDelta { delta, item_id, .. } => {
                let block_index = self.ensure_tool_block(out, &item_id, "custom_tool");
                self.streamed_tool_args.insert(item_id.clone());
                if !delta.is_empty() {
                    out.push(input_json_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::CustomToolCallInputDone { input, item_id, .. } => {
                if !self.streamed_tool_args.contains(&item_id) {
                    let block_index = self.ensure_tool_block(out, &item_id, "custom_tool");
                    if !input.is_empty() {
                        out.push(input_json_delta_event(block_index, input));
                    }
                }
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::McpCallArgumentsDelta { delta, item_id, .. } => {
                let block_index = self.ensure_tool_block(out, &item_id, "mcp_call");
                self.streamed_tool_args.insert(item_id.clone());
                if !delta.is_empty() {
                    out.push(input_json_delta_event(block_index, delta));
                }
            }
            ResponseStreamEvent::McpCallArgumentsDone {
                arguments, item_id, ..
            } => {
                if !self.streamed_tool_args.contains(&item_id) {
                    let block_index = self.ensure_tool_block(out, &item_id, "mcp_call");
                    if !arguments.is_empty() {
                        out.push(input_json_delta_event(block_index, arguments));
                    }
                }
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::McpCallInProgress { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "mcp_call");
            }
            ResponseStreamEvent::McpCallCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::McpCallFailed { item_id, .. } => {
                self.emit_text_block(out, format!("mcp_call_failed({item_id})"));
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::McpListToolsInProgress { item_id, .. } => {
                self.ensure_tool_block(out, &item_id, "mcp_list_tools");
            }
            ResponseStreamEvent::McpListToolsCompleted { item_id, .. } => {
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::McpListToolsFailed { item_id, .. } => {
                self.emit_text_block(out, format!("mcp_list_tools_failed({item_id})"));
                self.close_tool_block(out, &item_id);
            }
            ResponseStreamEvent::Error { error, .. } => {
                self.has_refusal = true;
                out.push(stream_error_event(error.message));
                self.stop_reason = Some(BetaStopReason::Refusal);
            }
            ResponseStreamEvent::Keepalive { .. } => {}
        }
    }

    pub fn finish(&mut self, out: &mut Vec<ClaudeStreamEvent>) {
        if self.is_finished() {
            return;
        }

        self.ensure_running(out);

        for block_index in std::mem::take(&mut self.open_text_blocks).into_values() {
            out.push(stop_block_event(block_index));
        }
        for block_index in std::mem::take(&mut self.open_thinking_blocks).into_values() {
            out.push(stop_block_event(block_index));
        }
        for block_index in std::mem::take(&mut self.open_summary_blocks).into_values() {
            out.push(stop_block_event(block_index));
        }
        for block_index in std::mem::take(&mut self.open_tool_blocks).into_values() {
            out.push(stop_block_event(block_index));
        }

        let final_stop_reason = self.stop_reason.clone().or({
            if self.has_tool_use {
                Some(BetaStopReason::ToolUse)
            } else if self.has_refusal {
                Some(BetaStopReason::Refusal)
            } else {
                Some(BetaStopReason::EndTurn)
            }
        });
        out.push(message_delta_event(
            final_stop_reason,
            self.input_tokens,
            self.cached_input_tokens,
            self.output_tokens,
        ));
        out.push(message_stop_event());
        self.state = StreamState::Finished;
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAiResponseToClaudeStream;
    use crate::claude::create_message::stream::{BetaRawContentBlockDelta, ClaudeStreamEvent};
    use crate::claude::create_message::types::BetaStopReason;
    use crate::openai::count_tokens::types as ot;
    use crate::openai::create_response::response::ResponseBody;
    use crate::openai::create_response::stream::ResponseStreamEvent;
    use crate::openai::create_response::types::{
        ResponseInputTokensDetails, ResponseObject, ResponseOutputTokensDetails, ResponseReasoning,
        ResponseServiceTier, ResponseStatus, ResponseTextConfig, ResponseToolChoice, ResponseUsage,
    };

    fn base_response() -> ResponseBody {
        ResponseBody {
            id: "resp_test".to_string(),
            created_at: 1_776_310_008,
            error: None,
            incomplete_details: None,
            instructions: Some(crate::openai::count_tokens::types::ResponseInput::Text(
                "test".to_string(),
            )),
            metadata: Default::default(),
            model: "gpt-5.4".to_string(),
            object: ResponseObject::Response,
            output: Vec::new(),
            parallel_tool_calls: true,
            temperature: 1.0,
            tool_choice: ResponseToolChoice::Options(ot::ResponseToolChoiceOptions::Auto),
            tools: Vec::new(),
            top_p: 0.98,
            background: Some(false),
            completed_at: None,
            conversation: None,
            max_output_tokens: None,
            max_tool_calls: None,
            output_text: None,
            previous_response_id: None,
            prompt: None,
            prompt_cache_key: None,
            prompt_cache_retention: None,
            reasoning: Some(ResponseReasoning {
                effort: Some(ot::ResponseReasoningEffort::Medium),
                generate_summary: None,
                summary: None,
            }),
            safety_identifier: None,
            service_tier: Some(ResponseServiceTier::Auto),
            status: Some(ResponseStatus::InProgress),
            text: Some(ResponseTextConfig {
                format: Some(ot::ResponseTextFormatConfig::Text(ot::ResponseFormatText {
                    type_: ot::ResponseFormatTextType::Text,
                })),
                verbosity: Some(ot::ResponseTextVerbosity::Medium),
            }),
            top_logprobs: Some(0),
            truncation: Some(crate::openai::create_response::types::ResponseTruncation::Disabled),
            usage: None,
            user: None,
        }
    }

    #[test]
    fn tool_calls_finish_with_tool_use_stop_reason() {
        let mut converter = OpenAiResponseToClaudeStream::default();
        let mut out = Vec::new();

        let created = base_response();
        converter.on_stream_event(
            ResponseStreamEvent::Created {
                response: created,
                sequence_number: 0,
            },
            &mut out,
        );

        converter.on_stream_event(
            ResponseStreamEvent::OutputItemAdded {
                item: crate::openai::create_response::types::ResponseOutputItem::FunctionToolCall(
                    crate::openai::count_tokens::types::ResponseFunctionToolCall {
                        arguments: String::new(),
                        call_id: "call_1".to_string(),
                        name: "Skill".to_string(),
                        type_: ot::ResponseFunctionToolCallType::FunctionCall,
                        id: Some("fc_1".to_string()),
                        status: Some(ot::ResponseItemStatus::InProgress),
                    },
                ),
                output_index: 0,
                sequence_number: 1,
            },
            &mut out,
        );

        converter.on_stream_event(
            ResponseStreamEvent::FunctionCallArgumentsDone {
                arguments: "{\"args\":\"\",\"skill\":\"superpowers:using-superpowers\"}"
                    .to_string(),
                item_id: "fc_1".to_string(),
                output_index: 0,
                sequence_number: 2,
                name: Some("Skill".to_string()),
            },
            &mut out,
        );

        let mut completed = base_response();
        completed.status = Some(ResponseStatus::Completed);
        completed.completed_at = Some(1_776_310_014);
        completed.usage = Some(ResponseUsage {
            input_tokens: 26_138,
            input_tokens_details: ResponseInputTokensDetails { cached_tokens: 0 },
            output_tokens: 85,
            output_tokens_details: ResponseOutputTokensDetails {
                reasoning_tokens: 59,
            },
            total_tokens: 26_223,
        });
        converter.on_stream_event(
            ResponseStreamEvent::Completed {
                response: completed,
                sequence_number: 3,
            },
            &mut out,
        );

        converter.finish(&mut out);

        let last_delta = out.iter().rev().find_map(|event| match event {
            ClaudeStreamEvent::MessageDelta { delta, usage, .. } => Some((
                delta.stop_reason.clone(),
                usage.input_tokens,
                usage.output_tokens,
            )),
            _ => None,
        });

        assert_eq!(
            last_delta,
            Some((Some(BetaStopReason::ToolUse), Some(26_138), 85))
        );
    }

    #[test]
    fn text_stream_events_do_not_duplicate_content() {
        let mut converter = OpenAiResponseToClaudeStream::default();
        let mut out = Vec::new();
        let item_id = "msg_1".to_string();

        converter.on_stream_event(
            ResponseStreamEvent::Created {
                response: base_response(),
                sequence_number: 0,
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::OutputItemAdded {
                item: crate::openai::create_response::types::ResponseOutputItem::Message(
                    ot::ResponseOutputMessage {
                        id: item_id.clone(),
                        content: Vec::new(),
                        role: ot::ResponseOutputMessageRole::Assistant,
                        phase: None,
                        status: ot::ResponseItemStatus::InProgress,
                        type_: ot::ResponseOutputMessageType::Message,
                    },
                ),
                output_index: 0,
                sequence_number: 1,
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::ContentPartAdded {
                content_index: 0,
                item_id: item_id.clone(),
                output_index: 0,
                part: crate::openai::create_response::stream::ResponseStreamContentPart::OutputText(
                    ot::ResponseOutputText {
                        annotations: Vec::new(),
                        logprobs: None,
                        text: String::new(),
                        type_: ot::ResponseOutputTextType::OutputText,
                    },
                ),
                sequence_number: 2,
            },
            &mut out,
        );
        for (sequence_number, delta) in ["{\"", "title", "\":\"", "Hello", "\"}"]
            .into_iter()
            .enumerate()
        {
            converter.on_stream_event(
                ResponseStreamEvent::OutputTextDelta {
                    content_index: 0,
                    delta: delta.to_string(),
                    item_id: item_id.clone(),
                    logprobs: None,
                    output_index: 0,
                    sequence_number: (sequence_number + 3) as u64,
                    obfuscation: None,
                },
                &mut out,
            );
        }
        converter.on_stream_event(
            ResponseStreamEvent::OutputTextDone {
                content_index: 0,
                item_id: item_id.clone(),
                logprobs: None,
                output_index: 0,
                sequence_number: 8,
                text: "{\"title\":\"Hello\"}".to_string(),
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::ContentPartDone {
                content_index: 0,
                item_id: item_id.clone(),
                output_index: 0,
                part: crate::openai::create_response::stream::ResponseStreamContentPart::OutputText(
                    ot::ResponseOutputText {
                        annotations: Vec::new(),
                        logprobs: None,
                        text: "{\"title\":\"Hello\"}".to_string(),
                        type_: ot::ResponseOutputTextType::OutputText,
                    },
                ),
                sequence_number: 9,
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::OutputItemDone {
                item: crate::openai::create_response::types::ResponseOutputItem::Message(
                    ot::ResponseOutputMessage {
                        id: item_id,
                        content: vec![ot::ResponseOutputContent::Text(ot::ResponseOutputText {
                            annotations: Vec::new(),
                            logprobs: None,
                            text: "{\"title\":\"Hello\"}".to_string(),
                            type_: ot::ResponseOutputTextType::OutputText,
                        })],
                        role: ot::ResponseOutputMessageRole::Assistant,
                        phase: None,
                        status: ot::ResponseItemStatus::Completed,
                        type_: ot::ResponseOutputMessageType::Message,
                    },
                ),
                output_index: 0,
                sequence_number: 10,
            },
            &mut out,
        );

        converter.finish(&mut out);

        let mut text_blocks = 0usize;
        let mut text_payload = String::new();
        for event in out {
            match event {
                ClaudeStreamEvent::ContentBlockStart {
                    content_block: crate::claude::create_message::types::BetaContentBlock::Text(_),
                    ..
                } => text_blocks += 1,
                ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::Text { text },
                    ..
                } => text_payload.push_str(&text),
                _ => {}
            }
        }

        assert_eq!(text_blocks, 1);
        assert_eq!(text_payload, "{\"title\":\"Hello\"}");
    }

    #[test]
    fn function_call_stream_events_do_not_duplicate_tool_payload() {
        let mut converter = OpenAiResponseToClaudeStream::default();
        let mut out = Vec::new();

        converter.on_stream_event(
            ResponseStreamEvent::Created {
                response: base_response(),
                sequence_number: 0,
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::OutputItemAdded {
                item: crate::openai::create_response::types::ResponseOutputItem::FunctionToolCall(
                    crate::openai::count_tokens::types::ResponseFunctionToolCall {
                        arguments: String::new(),
                        call_id: "call_1".to_string(),
                        name: "Skill".to_string(),
                        type_: ot::ResponseFunctionToolCallType::FunctionCall,
                        id: Some("fc_1".to_string()),
                        status: Some(ot::ResponseItemStatus::InProgress),
                    },
                ),
                output_index: 0,
                sequence_number: 1,
            },
            &mut out,
        );
        for (sequence_number, delta) in ["{\"", "args", "\":\"\"}"].into_iter().enumerate() {
            converter.on_stream_event(
                ResponseStreamEvent::FunctionCallArgumentsDelta {
                    delta: delta.to_string(),
                    item_id: "fc_1".to_string(),
                    output_index: 0,
                    sequence_number: (sequence_number + 2) as u64,
                    obfuscation: None,
                },
                &mut out,
            );
        }
        converter.on_stream_event(
            ResponseStreamEvent::FunctionCallArgumentsDone {
                arguments: "{\"args\":\"\"}".to_string(),
                item_id: "fc_1".to_string(),
                name: Some("Skill".to_string()),
                output_index: 0,
                sequence_number: 5,
            },
            &mut out,
        );
        converter.on_stream_event(
            ResponseStreamEvent::OutputItemDone {
                item: crate::openai::create_response::types::ResponseOutputItem::FunctionToolCall(
                    crate::openai::count_tokens::types::ResponseFunctionToolCall {
                        arguments: "{\"args\":\"\"}".to_string(),
                        call_id: "call_1".to_string(),
                        name: "Skill".to_string(),
                        type_: ot::ResponseFunctionToolCallType::FunctionCall,
                        id: Some("fc_1".to_string()),
                        status: Some(ot::ResponseItemStatus::Completed),
                    },
                ),
                output_index: 0,
                sequence_number: 6,
            },
            &mut out,
        );

        converter.finish(&mut out);

        let mut tool_blocks = 0usize;
        let mut tool_payload = String::new();
        for event in out {
            match event {
                ClaudeStreamEvent::ContentBlockStart {
                    content_block:
                        crate::claude::create_message::types::BetaContentBlock::ToolUse(_),
                    ..
                } => tool_blocks += 1,
                ClaudeStreamEvent::ContentBlockDelta {
                    delta: BetaRawContentBlockDelta::InputJson { partial_json },
                    ..
                } => tool_payload.push_str(&partial_json),
                _ => {}
            }
        }

        assert_eq!(tool_blocks, 1);
        assert_eq!(tool_payload, "{\"args\":\"\"}");
    }
}
