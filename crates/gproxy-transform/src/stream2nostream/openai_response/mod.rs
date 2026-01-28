use std::collections::BTreeMap;

use gproxy_protocol::openai::create_response::response::Response;
use gproxy_protocol::openai::create_response::stream::{
    ResponseCodeInterpreterCallCodeDeltaEvent, ResponseCodeInterpreterCallCodeDoneEvent,
    ResponseCompletedEvent, ResponseContentPartAddedEvent, ResponseContentPartDoneEvent,
    ResponseCustomToolCallInputDeltaEvent, ResponseCustomToolCallInputDoneEvent,
    ResponseFunctionCallArgumentsDeltaEvent, ResponseFunctionCallArgumentsDoneEvent,
    ResponseImageGenCallPartialImageEvent, ResponseMCPCallArgumentsDeltaEvent,
    ResponseMCPCallArgumentsDoneEvent, ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent,
    ResponseOutputTextAnnotationAddedEvent, ResponseReasoningSummaryPartAddedEvent,
    ResponseReasoningSummaryPartDoneEvent, ResponseReasoningSummaryTextDeltaEvent,
    ResponseReasoningSummaryTextDoneEvent, ResponseReasoningTextDeltaEvent,
    ResponseReasoningTextDoneEvent, ResponseRefusalDeltaEvent, ResponseRefusalDoneEvent,
    ResponseStreamEvent, ResponseTextDeltaEvent, ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    Annotation, CodeInterpreterToolCall, CodeInterpreterToolCallStatus,
    CodeInterpreterToolCallType, CustomToolCall, CustomToolCallType, FileSearchToolCallStatus,
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, ImageGenToolCallStatus,
    MCPToolCall, MCPToolCallStatus, MCPToolCallType, MessageStatus, OutputContent, OutputItem,
    OutputMessage, OutputMessageContent, OutputMessageRole, OutputMessageType, OutputTextContent,
    ReasoningContent, ReasoningItem, ReasoningItemStatus, ReasoningItemType, ReasoningTextContent,
    RefusalContent, ResponseStatus, SummaryPart, SummaryTextContent, WebSearchToolCallStatus,
};

#[derive(Debug, Clone)]
enum MessagePartState {
    Text(OutputTextContent),
    Refusal(RefusalContent),
}

#[derive(Debug, Clone)]
pub struct OpenAIResponseStreamToResponseState {
    response: Option<Response>,
    output_items: BTreeMap<i64, OutputItem>,
    message_parts: BTreeMap<(i64, i64), MessagePartState>,
    reasoning_contents: BTreeMap<(i64, i64), ReasoningContent>,
    reasoning_summaries: BTreeMap<(i64, i64), SummaryPart>,
}

impl OpenAIResponseStreamToResponseState {
    pub fn new() -> Self {
        Self {
            response: None,
            output_items: BTreeMap::new(),
            message_parts: BTreeMap::new(),
            reasoning_contents: BTreeMap::new(),
            reasoning_summaries: BTreeMap::new(),
        }
    }

    pub fn push_event(&mut self, event: ResponseStreamEvent) -> Option<Response> {
        match event {
            ResponseStreamEvent::Created(event) => {
                self.update_response(event.response);
                None
            }
            ResponseStreamEvent::Queued(event) => {
                self.update_response(event.response);
                None
            }
            ResponseStreamEvent::InProgress(event) => {
                self.update_response(event.response);
                None
            }
            ResponseStreamEvent::Completed(event) => Some(self.finish_from_response(event)),
            ResponseStreamEvent::Failed(event) => {
                Some(self.finish_from_response(ResponseCompletedEvent {
                    response: event.response,
                    sequence_number: event.sequence_number,
                }))
            }
            ResponseStreamEvent::Incomplete(event) => {
                Some(self.finish_from_response(ResponseCompletedEvent {
                    response: event.response,
                    sequence_number: event.sequence_number,
                }))
            }
            ResponseStreamEvent::OutputItemAdded(event) => {
                self.handle_output_item_added(event);
                None
            }
            ResponseStreamEvent::OutputItemDone(event) => {
                self.handle_output_item_done(event);
                None
            }
            ResponseStreamEvent::OutputTextDelta(event) => {
                self.handle_text_delta(event);
                None
            }
            ResponseStreamEvent::OutputTextDone(event) => {
                self.handle_text_done(event);
                None
            }
            ResponseStreamEvent::RefusalDelta(event) => {
                self.handle_refusal_delta(event);
                None
            }
            ResponseStreamEvent::RefusalDone(event) => {
                self.handle_refusal_done(event);
                None
            }
            ResponseStreamEvent::ContentPartAdded(event) => {
                self.handle_content_part_added(event);
                None
            }
            ResponseStreamEvent::ContentPartDone(event) => {
                self.handle_content_part_done(event);
                None
            }
            ResponseStreamEvent::OutputTextAnnotationAdded(event) => {
                self.handle_output_text_annotation_added(event);
                None
            }
            ResponseStreamEvent::ReasoningTextDelta(event) => {
                self.handle_reasoning_text_delta(event);
                None
            }
            ResponseStreamEvent::ReasoningTextDone(event) => {
                self.handle_reasoning_text_done(event);
                None
            }
            ResponseStreamEvent::ReasoningSummaryPartAdded(event) => {
                self.handle_reasoning_summary_part_added(event);
                None
            }
            ResponseStreamEvent::ReasoningSummaryPartDone(event) => {
                self.handle_reasoning_summary_part_done(event);
                None
            }
            ResponseStreamEvent::ReasoningSummaryTextDelta(event) => {
                self.handle_reasoning_summary_text_delta(event);
                None
            }
            ResponseStreamEvent::ReasoningSummaryTextDone(event) => {
                self.handle_reasoning_summary_text_done(event);
                None
            }
            ResponseStreamEvent::FunctionCallArgumentsDelta(event) => {
                self.handle_function_call_delta(event);
                None
            }
            ResponseStreamEvent::FunctionCallArgumentsDone(event) => {
                self.handle_function_call_done(event);
                None
            }
            ResponseStreamEvent::MCPCallArgumentsDelta(event) => {
                self.handle_mcp_call_delta(event);
                None
            }
            ResponseStreamEvent::MCPCallArgumentsDone(event) => {
                self.handle_mcp_call_done(event);
                None
            }
            ResponseStreamEvent::MCPCallInProgress(event) => {
                self.handle_mcp_call_status(
                    event.output_index,
                    event.item_id,
                    MCPToolCallStatus::InProgress,
                );
                None
            }
            ResponseStreamEvent::MCPCallCompleted(event) => {
                self.handle_mcp_call_status(
                    event.output_index,
                    event.item_id,
                    MCPToolCallStatus::Completed,
                );
                None
            }
            ResponseStreamEvent::MCPCallFailed(event) => {
                self.handle_mcp_call_status(
                    event.output_index,
                    event.item_id,
                    MCPToolCallStatus::Failed,
                );
                None
            }
            ResponseStreamEvent::CustomToolCallInputDelta(event) => {
                self.handle_custom_tool_call_delta(event);
                None
            }
            ResponseStreamEvent::CustomToolCallInputDone(event) => {
                self.handle_custom_tool_call_done(event);
                None
            }
            ResponseStreamEvent::FileSearchCallInProgress(event) => {
                self.handle_file_search_status(
                    event.output_index,
                    FileSearchToolCallStatus::InProgress,
                );
                None
            }
            ResponseStreamEvent::FileSearchCallSearching(event) => {
                self.handle_file_search_status(
                    event.output_index,
                    FileSearchToolCallStatus::Searching,
                );
                None
            }
            ResponseStreamEvent::FileSearchCallCompleted(event) => {
                self.handle_file_search_status(
                    event.output_index,
                    FileSearchToolCallStatus::Completed,
                );
                None
            }
            ResponseStreamEvent::WebSearchCallInProgress(event) => {
                self.handle_web_search_status(
                    event.output_index,
                    WebSearchToolCallStatus::InProgress,
                );
                None
            }
            ResponseStreamEvent::WebSearchCallSearching(event) => {
                self.handle_web_search_status(
                    event.output_index,
                    WebSearchToolCallStatus::Searching,
                );
                None
            }
            ResponseStreamEvent::WebSearchCallCompleted(event) => {
                self.handle_web_search_status(
                    event.output_index,
                    WebSearchToolCallStatus::Completed,
                );
                None
            }
            ResponseStreamEvent::ImageGenCallInProgress(event) => {
                self.handle_image_gen_status(
                    event.output_index,
                    event.item_id,
                    ImageGenToolCallStatus::InProgress,
                );
                None
            }
            ResponseStreamEvent::ImageGenCallGenerating(event) => {
                self.handle_image_gen_status(
                    event.output_index,
                    event.item_id,
                    ImageGenToolCallStatus::Generating,
                );
                None
            }
            ResponseStreamEvent::ImageGenCallCompleted(event) => {
                self.handle_image_gen_status(
                    event.output_index,
                    event.item_id,
                    ImageGenToolCallStatus::Completed,
                );
                None
            }
            ResponseStreamEvent::ImageGenCallPartialImage(event) => {
                self.handle_image_gen_partial(event);
                None
            }
            ResponseStreamEvent::CodeInterpreterCallInProgress(event) => {
                self.handle_code_interpreter_status(
                    event.output_index,
                    event.item_id,
                    CodeInterpreterToolCallStatus::InProgress,
                );
                None
            }
            ResponseStreamEvent::CodeInterpreterCallInterpreting(event) => {
                self.handle_code_interpreter_status(
                    event.output_index,
                    event.item_id,
                    CodeInterpreterToolCallStatus::Interpreting,
                );
                None
            }
            ResponseStreamEvent::CodeInterpreterCallCompleted(event) => {
                self.handle_code_interpreter_status(
                    event.output_index,
                    event.item_id,
                    CodeInterpreterToolCallStatus::Completed,
                );
                None
            }
            ResponseStreamEvent::CodeInterpreterCallCodeDelta(event) => {
                self.handle_code_interpreter_code_delta(event);
                None
            }
            ResponseStreamEvent::CodeInterpreterCallCodeDone(event) => {
                self.handle_code_interpreter_code_done(event);
                None
            }
            _ => None,
        }
    }

    pub fn finalize(mut self) -> Option<Response> {
        let mut response = self.response.take()?;
        self.apply_output_items(&mut response);
        Some(response)
    }

    pub fn finalize_on_eof(&mut self) -> Option<Response> {
        let mut response = self.response.take()?;
        let status = response.status.unwrap_or(ResponseStatus::InProgress);
        let status = match status {
            ResponseStatus::Completed
            | ResponseStatus::Failed
            | ResponseStatus::Cancelled
            | ResponseStatus::Incomplete => status,
            ResponseStatus::Queued | ResponseStatus::InProgress => ResponseStatus::Incomplete,
        };
        response.status = Some(status);
        self.apply_output_items(&mut response);
        Some(response)
    }

    fn update_response(&mut self, response: Response) {
        self.response = Some(response);
    }

    fn handle_output_item_added(&mut self, event: ResponseOutputItemAddedEvent) {
        self.merge_output_item(event.output_index, event.item);
        self.sync_message_content(event.output_index);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_output_item_done(&mut self, event: ResponseOutputItemDoneEvent) {
        self.merge_output_item(event.output_index, event.item);
        self.sync_message_content(event.output_index);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_text_delta(&mut self, event: ResponseTextDeltaEvent) {
        self.ensure_message(event.output_index, &event.item_id);
        let key = (event.output_index, event.content_index);
        let entry = self.message_parts.entry(key).or_insert_with(|| {
            MessagePartState::Text(OutputTextContent {
                text: String::new(),
                annotations: Vec::new(),
                logprobs: None,
            })
        });

        if let MessagePartState::Text(text) = entry {
            text.text.push_str(&event.delta);
            // `ResponseLogProb` doesn't map cleanly to `LogProb` (bytes missing).
        }
        self.sync_message_content(event.output_index);
    }

    fn handle_text_done(&mut self, event: ResponseTextDoneEvent) {
        self.ensure_message(event.output_index, &event.item_id);
        let key = (event.output_index, event.content_index);
        let entry = self.message_parts.entry(key).or_insert_with(|| {
            MessagePartState::Text(OutputTextContent {
                text: String::new(),
                annotations: Vec::new(),
                logprobs: None,
            })
        });

        if let MessagePartState::Text(text) = entry {
            text.text = event.text;
            // `ResponseLogProb` doesn't map cleanly to `LogProb` (bytes missing).
        }
        self.sync_message_content(event.output_index);
    }

    fn handle_refusal_delta(&mut self, event: ResponseRefusalDeltaEvent) {
        self.ensure_message(event.output_index, &event.item_id);
        let key = (event.output_index, event.content_index);
        let entry = self.message_parts.entry(key).or_insert_with(|| {
            MessagePartState::Refusal(RefusalContent {
                refusal: String::new(),
            })
        });

        if let MessagePartState::Refusal(refusal) = entry {
            refusal.refusal.push_str(&event.delta);
        }
        self.sync_message_content(event.output_index);
    }

    fn handle_refusal_done(&mut self, event: ResponseRefusalDoneEvent) {
        self.ensure_message(event.output_index, &event.item_id);
        let key = (event.output_index, event.content_index);
        self.message_parts.insert(
            key,
            MessagePartState::Refusal(RefusalContent {
                refusal: event.refusal,
            }),
        );
        self.sync_message_content(event.output_index);
    }

    fn handle_content_part_added(&mut self, event: ResponseContentPartAddedEvent) {
        self.apply_output_content(event.output_index, event.content_index, event.part);
    }

    fn handle_content_part_done(&mut self, event: ResponseContentPartDoneEvent) {
        self.apply_output_content(event.output_index, event.content_index, event.part);
    }

    fn handle_output_text_annotation_added(
        &mut self,
        event: ResponseOutputTextAnnotationAddedEvent,
    ) {
        let key = (event.output_index, event.content_index);
        let entry = self.message_parts.entry(key).or_insert_with(|| {
            MessagePartState::Text(OutputTextContent {
                text: String::new(),
                annotations: Vec::new(),
                logprobs: None,
            })
        });

        if let MessagePartState::Text(text) = entry {
            push_annotation(
                &mut text.annotations,
                event.annotation_index,
                event.annotation,
            );
        }
        self.sync_message_content(event.output_index);
    }

    fn handle_reasoning_text_delta(&mut self, event: ResponseReasoningTextDeltaEvent) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        let key = (event.output_index, event.content_index);
        let entry = self.reasoning_contents.entry(key).or_insert_with(|| {
            ReasoningContent::ReasoningText(ReasoningTextContent {
                text: String::new(),
            })
        });

        let ReasoningContent::ReasoningText(text) = entry;
        text.text.push_str(&event.delta);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_reasoning_text_done(&mut self, event: ResponseReasoningTextDoneEvent) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        self.reasoning_contents.insert(
            (event.output_index, event.content_index),
            ReasoningContent::ReasoningText(ReasoningTextContent { text: event.text }),
        );
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_reasoning_summary_part_added(
        &mut self,
        event: ResponseReasoningSummaryPartAddedEvent,
    ) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        self.reasoning_summaries
            .insert((event.output_index, event.summary_index), event.part);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_reasoning_summary_part_done(&mut self, event: ResponseReasoningSummaryPartDoneEvent) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        self.reasoning_summaries
            .insert((event.output_index, event.summary_index), event.part);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_reasoning_summary_text_delta(
        &mut self,
        event: ResponseReasoningSummaryTextDeltaEvent,
    ) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        let key = (event.output_index, event.summary_index);
        let entry = self.reasoning_summaries.entry(key).or_insert_with(|| {
            SummaryPart::SummaryText(SummaryTextContent {
                text: String::new(),
            })
        });

        let SummaryPart::SummaryText(summary) = entry;
        summary.text.push_str(&event.delta);
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_reasoning_summary_text_done(&mut self, event: ResponseReasoningSummaryTextDoneEvent) {
        self.ensure_reasoning_item(event.output_index, &event.item_id);
        self.reasoning_summaries.insert(
            (event.output_index, event.summary_index),
            SummaryPart::SummaryText(SummaryTextContent { text: event.text }),
        );
        self.sync_reasoning_item(event.output_index);
    }

    fn handle_function_call_delta(&mut self, event: ResponseFunctionCallArgumentsDeltaEvent) {
        self.with_function_tool_call_mut(event.output_index, &event.item_id, None, |function| {
            function.arguments.push_str(&event.delta);
            if function.status.is_none() {
                function.status = Some(FunctionCallItemStatus::InProgress);
            }
        });
    }

    fn handle_function_call_done(&mut self, event: ResponseFunctionCallArgumentsDoneEvent) {
        self.with_function_tool_call_mut(
            event.output_index,
            &event.item_id,
            Some(event.name),
            |function| {
                function.arguments = event.arguments;
                function.status = Some(FunctionCallItemStatus::Completed);
            },
        );
    }

    fn handle_mcp_call_delta(&mut self, event: ResponseMCPCallArgumentsDeltaEvent) {
        self.with_mcp_tool_call_mut(event.output_index, &event.item_id, |mcp| {
            mcp.arguments.push_str(&event.delta);
            if matches!(mcp.status, MCPToolCallStatus::InProgress) {
                mcp.status = MCPToolCallStatus::Calling;
            }
        });
    }

    fn handle_mcp_call_done(&mut self, event: ResponseMCPCallArgumentsDoneEvent) {
        self.with_mcp_tool_call_mut(event.output_index, &event.item_id, |mcp| {
            mcp.arguments = event.arguments;
            if matches!(
                mcp.status,
                MCPToolCallStatus::InProgress | MCPToolCallStatus::Calling
            ) {
                mcp.status = MCPToolCallStatus::Calling;
            }
        });
    }

    fn handle_mcp_call_status(
        &mut self,
        output_index: i64,
        item_id: String,
        status: MCPToolCallStatus,
    ) {
        self.with_mcp_tool_call_mut(output_index, &item_id, |mcp| {
            mcp.status = status;
        });
    }

    fn handle_custom_tool_call_delta(&mut self, event: ResponseCustomToolCallInputDeltaEvent) {
        self.with_custom_tool_call_mut(event.output_index, &event.item_id, |custom| {
            custom.input.push_str(&event.delta);
        });
    }

    fn handle_custom_tool_call_done(&mut self, event: ResponseCustomToolCallInputDoneEvent) {
        self.with_custom_tool_call_mut(event.output_index, &event.item_id, |custom| {
            custom.input = event.input;
        });
    }

    fn handle_file_search_status(&mut self, output_index: i64, status: FileSearchToolCallStatus) {
        if let Some(OutputItem::FileSearch(call)) = self.output_items.get_mut(&output_index) {
            call.status = status;
        }
    }

    fn handle_web_search_status(&mut self, output_index: i64, status: WebSearchToolCallStatus) {
        if let Some(OutputItem::WebSearch(call)) = self.output_items.get_mut(&output_index) {
            call.status = status;
        }
    }

    fn handle_image_gen_status(
        &mut self,
        output_index: i64,
        item_id: String,
        status: ImageGenToolCallStatus,
    ) {
        if let Some(OutputItem::ImageGen(call)) = self.output_items.get_mut(&output_index) {
            call.status = status;
            return;
        }

        self.output_items.insert(
            output_index,
            OutputItem::ImageGen(gproxy_protocol::openai::create_response::types::ImageGenToolCall {
                r#type: gproxy_protocol::openai::create_response::types::ImageGenToolCallType::ImageGenerationCall,
                id: item_id,
                status,
                result: None,
            }),
        );
    }

    fn handle_image_gen_partial(&mut self, event: ResponseImageGenCallPartialImageEvent) {
        if let Some(OutputItem::ImageGen(call)) = self.output_items.get_mut(&event.output_index) {
            let result = call.result.get_or_insert_with(String::new);
            result.push_str(&event.partial_image_b64);
            return;
        }

        self.output_items.insert(
            event.output_index,
            OutputItem::ImageGen(gproxy_protocol::openai::create_response::types::ImageGenToolCall {
                r#type: gproxy_protocol::openai::create_response::types::ImageGenToolCallType::ImageGenerationCall,
                id: event.item_id,
                status: ImageGenToolCallStatus::Generating,
                result: Some(event.partial_image_b64),
            }),
        );
    }

    fn handle_code_interpreter_status(
        &mut self,
        output_index: i64,
        item_id: String,
        status: CodeInterpreterToolCallStatus,
    ) {
        self.ensure_code_interpreter_call(output_index, &item_id);
        if let Some(OutputItem::CodeInterpreter(call)) = self.output_items.get_mut(&output_index) {
            call.status = status;
        }
    }

    fn handle_code_interpreter_code_delta(
        &mut self,
        event: ResponseCodeInterpreterCallCodeDeltaEvent,
    ) {
        self.ensure_code_interpreter_call(event.output_index, &event.item_id);
        if let Some(OutputItem::CodeInterpreter(call)) =
            self.output_items.get_mut(&event.output_index)
        {
            let code = call.code.get_or_insert_with(String::new);
            code.push_str(&event.delta);
        }
    }

    fn handle_code_interpreter_code_done(
        &mut self,
        event: ResponseCodeInterpreterCallCodeDoneEvent,
    ) {
        self.ensure_code_interpreter_call(event.output_index, &event.item_id);
        if let Some(OutputItem::CodeInterpreter(call)) =
            self.output_items.get_mut(&event.output_index)
        {
            call.code = Some(event.code);
        }
    }

    fn apply_output_content(&mut self, output_index: i64, content_index: i64, part: OutputContent) {
        match part {
            OutputContent::OutputText(text) => {
                self.message_parts
                    .insert((output_index, content_index), MessagePartState::Text(text));
                self.sync_message_content(output_index);
            }
            OutputContent::Refusal(refusal) => {
                self.message_parts.insert(
                    (output_index, content_index),
                    MessagePartState::Refusal(refusal),
                );
                self.sync_message_content(output_index);
            }
            OutputContent::ReasoningText(_) => {
                // Reasoning content belongs to a reasoning item, not an output message.
            }
        }
    }

    fn ensure_message(&mut self, output_index: i64, item_id: &str) {
        let entry = self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::Message(OutputMessage {
                id: item_id.to_string(),
                r#type: OutputMessageType::Message,
                role: OutputMessageRole::Assistant,
                content: Vec::new(),
                status: MessageStatus::InProgress,
            })
        });

        if let OutputItem::Message(message) = entry
            && message.id.is_empty()
        {
            message.id = item_id.to_string();
        }
    }

    fn sync_message_content(&mut self, output_index: i64) {
        if !self.has_message_parts(output_index) {
            return;
        }
        let content = self.build_message_content(output_index);
        if let Some(OutputItem::Message(message)) = self.output_items.get_mut(&output_index) {
            message.content = content;
        }
    }

    fn ensure_reasoning_item(&mut self, output_index: i64, item_id: &str) {
        let entry = self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::Reasoning(ReasoningItem {
                r#type: ReasoningItemType::Reasoning,
                id: item_id.to_string(),
                encrypted_content: None,
                summary: Vec::new(),
                content: Vec::new(),
                status: Some(ReasoningItemStatus::InProgress),
            })
        });

        if let OutputItem::Reasoning(item) = entry {
            if item.id.is_empty() {
                item.id = item_id.to_string();
            }
            if item.status.is_none() {
                item.status = Some(ReasoningItemStatus::InProgress);
            }
        }
    }

    fn sync_reasoning_item(&mut self, output_index: i64) {
        if !self.has_reasoning_parts(output_index) {
            return;
        }
        let content = self.build_reasoning_content(output_index);
        let summary = self.build_reasoning_summary(output_index);
        if let Some(OutputItem::Reasoning(item)) = self.output_items.get_mut(&output_index) {
            item.content = content;
            item.summary = summary;
        }
    }

    fn has_reasoning_parts(&self, output_index: i64) -> bool {
        self.reasoning_contents
            .range((output_index, i64::MIN)..=(output_index, i64::MAX))
            .next()
            .is_some()
            || self
                .reasoning_summaries
                .range((output_index, i64::MIN)..=(output_index, i64::MAX))
                .next()
                .is_some()
    }

    fn build_reasoning_content(&self, output_index: i64) -> Vec<ReasoningContent> {
        self.reasoning_contents
            .range((output_index, i64::MIN)..=(output_index, i64::MAX))
            .map(|(_, part)| part.clone())
            .collect()
    }

    fn build_reasoning_summary(&self, output_index: i64) -> Vec<SummaryPart> {
        self.reasoning_summaries
            .range((output_index, i64::MIN)..=(output_index, i64::MAX))
            .map(|(_, part)| part.clone())
            .collect()
    }

    fn with_function_tool_call_mut<F>(
        &mut self,
        output_index: i64,
        item_id: &str,
        name: Option<String>,
        mutator: F,
    ) where
        F: FnOnce(&mut FunctionToolCall),
    {
        let entry = self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(item_id.to_string()),
                call_id: item_id.to_string(),
                name: name.clone().unwrap_or_else(|| "function".to_string()),
                arguments: String::new(),
                status: Some(FunctionCallItemStatus::InProgress),
            })
        });

        if let OutputItem::Function(function) = entry {
            if function.id.is_none() {
                function.id = Some(item_id.to_string());
            }
            if function.call_id.is_empty() {
                function.call_id = item_id.to_string();
            }
            if function.name.is_empty() {
                if let Some(name) = name {
                    function.name = name;
                }
            } else if let Some(name) = name {
                function.name = name;
            }
            mutator(function);
        }
    }

    fn with_mcp_tool_call_mut<F>(&mut self, output_index: i64, item_id: &str, mutator: F)
    where
        F: FnOnce(&mut MCPToolCall),
    {
        let entry = self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::MCPCall(MCPToolCall {
                r#type: MCPToolCallType::MCPCall,
                id: item_id.to_string(),
                server_label: "unknown".to_string(),
                name: "unknown".to_string(),
                arguments: String::new(),
                output: None,
                error: None,
                status: MCPToolCallStatus::InProgress,
                approval_request_id: None,
            })
        });

        if let OutputItem::MCPCall(mcp) = entry {
            if mcp.id.is_empty() {
                mcp.id = item_id.to_string();
            }
            mutator(mcp);
        }
    }

    fn with_custom_tool_call_mut<F>(&mut self, output_index: i64, item_id: &str, mutator: F)
    where
        F: FnOnce(&mut CustomToolCall),
    {
        let entry = self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::CustomToolCall(CustomToolCall {
                r#type: CustomToolCallType::CustomToolCall,
                id: Some(item_id.to_string()),
                call_id: item_id.to_string(),
                name: "custom_tool".to_string(),
                input: String::new(),
            })
        });

        if let OutputItem::CustomToolCall(custom) = entry {
            if custom.id.is_none() {
                custom.id = Some(item_id.to_string());
            }
            if custom.call_id.is_empty() {
                custom.call_id = item_id.to_string();
            }
            mutator(custom);
        }
    }

    fn ensure_code_interpreter_call(&mut self, output_index: i64, item_id: &str) {
        self.output_items.entry(output_index).or_insert_with(|| {
            OutputItem::CodeInterpreter(CodeInterpreterToolCall {
                r#type: CodeInterpreterToolCallType::CodeInterpreterCall,
                id: item_id.to_string(),
                status: CodeInterpreterToolCallStatus::InProgress,
                container_id: "unknown".to_string(),
                code: None,
                outputs: None,
            })
        });
    }

    fn merge_output_item(&mut self, output_index: i64, incoming: OutputItem) {
        let merged = match self.output_items.remove(&output_index) {
            Some(existing) => merge_output_item(existing, incoming),
            None => incoming,
        };
        self.output_items.insert(output_index, merged);
    }

    fn has_message_parts(&self, output_index: i64) -> bool {
        self.message_parts
            .range((output_index, i64::MIN)..=(output_index, i64::MAX))
            .next()
            .is_some()
    }

    fn build_message_content(&self, output_index: i64) -> Vec<OutputMessageContent> {
        self.message_parts
            .range((output_index, i64::MIN)..=(output_index, i64::MAX))
            .map(|(_, part)| match part {
                MessagePartState::Text(text) => OutputMessageContent::OutputText(text.clone()),
                MessagePartState::Refusal(refusal) => {
                    OutputMessageContent::Refusal(refusal.clone())
                }
            })
            .collect()
    }

    fn finish_from_response(&mut self, event: ResponseCompletedEvent) -> Response {
        let mut response = event.response;
        self.response = Some(response.clone());
        self.apply_output_items(&mut response);
        response
    }

    fn apply_output_items(&self, response: &mut Response) {
        if !self.output_items.is_empty() {
            let mut ordered: Vec<(i64, OutputItem)> = self
                .output_items
                .iter()
                .map(|(index, item)| (*index, item.clone()))
                .collect();
            ordered.sort_by_key(|(index, _)| *index);

            let mut output = Vec::with_capacity(ordered.len());
            for (index, mut item) in ordered {
                match &mut item {
                    OutputItem::Message(message) => {
                        if self.has_message_parts(index) {
                            message.content = self.build_message_content(index);
                        }
                    }
                    OutputItem::Reasoning(reasoning) => {
                        if self.has_reasoning_parts(index) {
                            reasoning.content = self.build_reasoning_content(index);
                            reasoning.summary = self.build_reasoning_summary(index);
                        }
                    }
                    _ => {}
                }
                output.push(item);
            }
            response.output = output;
        }

        if let Some(status) = response.status {
            for item in &mut response.output {
                if let OutputItem::Reasoning(reasoning) = item {
                    reasoning.status = Some(infer_reasoning_status(reasoning.status, status));
                }
            }
        }

        if response.output_text.is_none() {
            response.output_text = extract_output_text(&response.output);
        }
    }
}

impl Default for OpenAIResponseStreamToResponseState {
    fn default() -> Self {
        Self::new()
    }
}

fn push_annotation(annotations: &mut Vec<Annotation>, index: i64, annotation: Annotation) {
    if index < 0 {
        return;
    }
    let index = index as usize;
    if index < annotations.len() {
        annotations[index] = annotation;
    } else if index == annotations.len() {
        annotations.push(annotation);
    } else {
        // Annotation indexes can be sparse; keep order but append when missing.
        annotations.push(annotation);
    }
}

fn extract_output_text(output: &[OutputItem]) -> Option<String> {
    for item in output {
        if let OutputItem::Message(message) = item {
            for content in &message.content {
                if let OutputMessageContent::OutputText(text) = content
                    && !text.text.is_empty()
                {
                    return Some(text.text.clone());
                }
            }
        }
    }
    None
}

fn merge_output_item(existing: OutputItem, incoming: OutputItem) -> OutputItem {
    match (existing, incoming) {
        (OutputItem::Message(mut old), OutputItem::Message(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            old.status = prefer_message_status(old.status, new.status);
            if !new.content.is_empty() {
                old.content = new.content;
            }
            OutputItem::Message(old)
        }
        (OutputItem::Reasoning(mut old), OutputItem::Reasoning(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            if new.encrypted_content.is_some() {
                old.encrypted_content = new.encrypted_content;
            }
            if !new.content.is_empty() {
                old.content = new.content;
            }
            if !new.summary.is_empty() {
                old.summary = new.summary;
            }
            old.status = prefer_reasoning_status(old.status, new.status);
            OutputItem::Reasoning(old)
        }
        (OutputItem::FileSearch(mut old), OutputItem::FileSearch(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            if !new.queries.is_empty() {
                old.queries = new.queries;
            }
            if new.results.is_some() {
                old.results = new.results;
            }
            old.status = prefer_file_search_status(old.status, new.status);
            OutputItem::FileSearch(old)
        }
        (OutputItem::WebSearch(mut old), OutputItem::WebSearch(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            old.action = new.action;
            old.status = prefer_web_search_status(old.status, new.status);
            OutputItem::WebSearch(old)
        }
        (OutputItem::ImageGen(mut old), OutputItem::ImageGen(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            if new.result.is_some() {
                old.result = new.result;
            }
            old.status = prefer_image_gen_status(old.status, new.status);
            OutputItem::ImageGen(old)
        }
        (OutputItem::CodeInterpreter(mut old), OutputItem::CodeInterpreter(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            if !new.container_id.is_empty() {
                old.container_id = new.container_id;
            }
            if new.code.is_some() {
                old.code = new.code;
            }
            if new.outputs.is_some() {
                old.outputs = new.outputs;
            }
            old.status = prefer_code_interpreter_status(old.status, new.status);
            OutputItem::CodeInterpreter(old)
        }
        (OutputItem::Function(mut old), OutputItem::Function(new)) => {
            if new.id.is_some() {
                old.id = new.id;
            }
            if !new.call_id.is_empty() {
                old.call_id = new.call_id;
            }
            if !new.name.is_empty() {
                old.name = new.name;
            }
            if !new.arguments.is_empty() {
                old.arguments = new.arguments;
            }
            old.status = prefer_function_status(old.status, new.status);
            OutputItem::Function(old)
        }
        (OutputItem::CustomToolCall(mut old), OutputItem::CustomToolCall(new)) => {
            if new.id.is_some() {
                old.id = new.id;
            }
            if !new.call_id.is_empty() {
                old.call_id = new.call_id;
            }
            if !new.name.is_empty() {
                old.name = new.name;
            }
            if !new.input.is_empty() {
                old.input = new.input;
            }
            OutputItem::CustomToolCall(old)
        }
        (OutputItem::MCPCall(mut old), OutputItem::MCPCall(new)) => {
            if !new.id.is_empty() {
                old.id = new.id;
            }
            if !new.server_label.is_empty() {
                old.server_label = new.server_label;
            }
            if !new.name.is_empty() {
                old.name = new.name;
            }
            if !new.arguments.is_empty() {
                old.arguments = new.arguments;
            }
            if new.output.is_some() {
                old.output = new.output;
            }
            if new.error.is_some() {
                old.error = new.error;
            }
            if new.approval_request_id.is_some() {
                old.approval_request_id = new.approval_request_id;
            }
            old.status = prefer_mcp_status(old.status, new.status);
            OutputItem::MCPCall(old)
        }
        (_, incoming) => incoming,
    }
}

fn infer_reasoning_status(
    current: Option<ReasoningItemStatus>,
    response_status: ResponseStatus,
) -> ReasoningItemStatus {
    match response_status {
        ResponseStatus::Completed => current.unwrap_or(ReasoningItemStatus::Completed),
        ResponseStatus::Incomplete | ResponseStatus::Failed | ResponseStatus::Cancelled => {
            ReasoningItemStatus::Incomplete
        }
        ResponseStatus::InProgress | ResponseStatus::Queued => {
            current.unwrap_or(ReasoningItemStatus::InProgress)
        }
    }
}

fn prefer_message_status(current: MessageStatus, incoming: MessageStatus) -> MessageStatus {
    prefer_status(current, incoming, message_status_rank)
}

fn prefer_reasoning_status(
    current: Option<ReasoningItemStatus>,
    incoming: Option<ReasoningItemStatus>,
) -> Option<ReasoningItemStatus> {
    match (current, incoming) {
        (Some(current), Some(incoming)) => {
            Some(prefer_status(current, incoming, reasoning_status_rank))
        }
        (None, Some(incoming)) => Some(incoming),
        (Some(current), None) => Some(current),
        (None, None) => None,
    }
}

fn prefer_function_status(
    current: Option<FunctionCallItemStatus>,
    incoming: Option<FunctionCallItemStatus>,
) -> Option<FunctionCallItemStatus> {
    match (current, incoming) {
        (Some(current), Some(incoming)) => {
            Some(prefer_status(current, incoming, function_status_rank))
        }
        (None, Some(incoming)) => Some(incoming),
        (Some(current), None) => Some(current),
        (None, None) => None,
    }
}

fn prefer_file_search_status(
    current: FileSearchToolCallStatus,
    incoming: FileSearchToolCallStatus,
) -> FileSearchToolCallStatus {
    prefer_status(current, incoming, file_search_status_rank)
}

fn prefer_web_search_status(
    current: WebSearchToolCallStatus,
    incoming: WebSearchToolCallStatus,
) -> WebSearchToolCallStatus {
    prefer_status(current, incoming, web_search_status_rank)
}

fn prefer_image_gen_status(
    current: ImageGenToolCallStatus,
    incoming: ImageGenToolCallStatus,
) -> ImageGenToolCallStatus {
    prefer_status(current, incoming, image_gen_status_rank)
}

fn prefer_code_interpreter_status(
    current: CodeInterpreterToolCallStatus,
    incoming: CodeInterpreterToolCallStatus,
) -> CodeInterpreterToolCallStatus {
    prefer_status(current, incoming, code_interpreter_status_rank)
}

fn prefer_mcp_status(current: MCPToolCallStatus, incoming: MCPToolCallStatus) -> MCPToolCallStatus {
    prefer_status(current, incoming, mcp_status_rank)
}

fn prefer_status<T: Copy + Eq>(current: T, incoming: T, ranker: fn(T) -> i32) -> T {
    if ranker(incoming) >= ranker(current) {
        incoming
    } else {
        current
    }
}

fn message_status_rank(status: MessageStatus) -> i32 {
    match status {
        MessageStatus::InProgress => 0,
        MessageStatus::Incomplete => 1,
        MessageStatus::Completed => 2,
    }
}

fn reasoning_status_rank(status: ReasoningItemStatus) -> i32 {
    match status {
        ReasoningItemStatus::InProgress => 0,
        ReasoningItemStatus::Incomplete => 1,
        ReasoningItemStatus::Completed => 2,
    }
}

fn function_status_rank(status: FunctionCallItemStatus) -> i32 {
    match status {
        FunctionCallItemStatus::InProgress => 0,
        FunctionCallItemStatus::Incomplete => 1,
        FunctionCallItemStatus::Completed => 2,
    }
}

fn file_search_status_rank(status: FileSearchToolCallStatus) -> i32 {
    match status {
        FileSearchToolCallStatus::InProgress => 0,
        FileSearchToolCallStatus::Searching => 1,
        FileSearchToolCallStatus::Incomplete => 2,
        FileSearchToolCallStatus::Completed => 3,
        FileSearchToolCallStatus::Failed => 4,
    }
}

fn web_search_status_rank(status: WebSearchToolCallStatus) -> i32 {
    match status {
        WebSearchToolCallStatus::InProgress => 0,
        WebSearchToolCallStatus::Searching => 1,
        WebSearchToolCallStatus::Completed => 3,
        WebSearchToolCallStatus::Failed => 4,
    }
}

fn image_gen_status_rank(status: ImageGenToolCallStatus) -> i32 {
    match status {
        ImageGenToolCallStatus::InProgress => 0,
        ImageGenToolCallStatus::Generating => 1,
        ImageGenToolCallStatus::Completed => 3,
        ImageGenToolCallStatus::Failed => 4,
    }
}

fn code_interpreter_status_rank(status: CodeInterpreterToolCallStatus) -> i32 {
    match status {
        CodeInterpreterToolCallStatus::InProgress => 0,
        CodeInterpreterToolCallStatus::Interpreting => 1,
        CodeInterpreterToolCallStatus::Incomplete => 2,
        CodeInterpreterToolCallStatus::Completed => 3,
        CodeInterpreterToolCallStatus::Failed => 4,
    }
}

fn mcp_status_rank(status: MCPToolCallStatus) -> i32 {
    match status {
        MCPToolCallStatus::InProgress => 0,
        MCPToolCallStatus::Calling => 1,
        MCPToolCallStatus::Incomplete => 2,
        MCPToolCallStatus::Completed => 3,
        MCPToolCallStatus::Failed => 4,
    }
}
