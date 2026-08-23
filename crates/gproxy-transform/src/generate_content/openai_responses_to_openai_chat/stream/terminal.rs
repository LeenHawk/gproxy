use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, event, message_item, message_part, reasoning_item, tool_item};
use super::{Item, State, Tool, ToolKind};

enum Done {
    Text(Item),
    Reasoning(Item),
    Tool(Tool),
}

impl Done {
    fn index(&self) -> u32 {
        match self {
            Self::Text(item) | Self::Reasoning(item) => item.index,
            Self::Tool(item) => item.index,
        }
    }
}

impl State {
    pub(super) fn stop(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped || !self.started {
            return if self.stopped {
                Ok(Vec::new())
            } else {
                Err(TransformError::IncompleteStream)
            };
        }
        let mut output = Vec::new();
        let mut done = Vec::new();
        done.extend(self.text.clone().map(Done::Text));
        done.extend(self.reasoning.clone().map(Done::Reasoning));
        done.extend(self.tools.values().cloned().map(Done::Tool));
        done.sort_by_key(Done::index);
        for item in done {
            output.extend(self.done(item)?);
        }
        let finish_reason = self.finish_reason.as_ref().ok_or_else(|| {
            TransformError::shape("Chat stream", "DONE before a choice finish_reason")
        })?;
        let status = match finish_reason {
            openai::ChatFinishReason::Length | openai::ChatFinishReason::ContentFilter => {
                openai::ResponseStatus::Incomplete
            }
            openai::ChatFinishReason::Stop
            | openai::ChatFinishReason::ToolCalls
            | openai::ChatFinishReason::FunctionCall => openai::ResponseStatus::Completed,
            openai::ChatFinishReason::Unknown(value) => {
                return Err(TransformError::unsupported("Chat finish reason", value));
            }
        };
        let terminal = match status {
            openai::ResponseStatus::Incomplete => {
                openai::ResponseStreamEventTypeKnown::ResponseIncomplete
            }
            openai::ResponseStatus::Completed => {
                openai::ResponseStreamEventTypeKnown::ResponseCompleted
            }
            openai::ResponseStatus::Failed
            | openai::ResponseStatus::InProgress
            | openai::ResponseStatus::Cancelled
            | openai::ResponseStatus::Queued => {
                return Err(TransformError::shape(
                    "Responses stream",
                    "unsupported synthesized terminal status",
                ));
            }
            openai::ResponseStatus::Unknown(value) => {
                return Err(TransformError::unsupported("Responses status", value));
            }
        };
        let response = self.response(status)?;
        output.push(self.emit(terminal, Some(Box::new(response)), None, None, None, None)?);
        self.stopped = true;
        Ok(output)
    }

    fn done(&mut self, done: Done) -> Result<Vec<Bytes>, TransformError> {
        match done {
            Done::Text(item) => self.text_done(item),
            Done::Reasoning(item) => self.reasoning_done(item),
            Done::Tool(item) => self.tool_done(item),
        }
    }

    fn text_done(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let mut text_done = event(openai::ResponseStreamEventTypeKnown::ResponseOutputTextDone);
        text_done.sequence_number = Some(self.next_sequence());
        text_done.item_id = Some(item.id.clone());
        text_done.output_index = Some(item.index);
        text_done.content_index = Some(0);
        text_done.text = Some(item.text.clone());
        let part = openai::ResponseContentPart::OutputText(message_part(&item));
        Ok(vec![
            emit(text_done)?,
            self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseContentPartDone,
                None,
                None,
                Some(item.index),
                Some(item.id.clone()),
                Some(part),
            )?,
            self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone,
                None,
                Some(Box::new(message_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::Completed,
                ))),
                Some(item.index),
                None,
                None,
            )?,
        ])
    }

    fn reasoning_done(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let mut done = event(openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDone);
        done.sequence_number = Some(self.next_sequence());
        done.item_id = Some(item.id.clone());
        done.output_index = Some(item.index);
        done.content_index = Some(0);
        done.text = Some(item.text.clone());
        Ok(vec![
            emit(done)?,
            self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone,
                None,
                Some(Box::new(reasoning_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::Completed,
                ))),
                Some(item.index),
                None,
                None,
            )?,
        ])
    }

    fn tool_done(&mut self, item: Tool) -> Result<Vec<Bytes>, TransformError> {
        let mut done = event(match item.kind {
            ToolKind::Function => {
                openai::ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone
            }
            ToolKind::Custom => {
                openai::ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone
            }
        });
        done.sequence_number = Some(self.next_sequence());
        done.item_id = Some(item.id.clone());
        done.output_index = Some(item.index);
        match item.kind {
            ToolKind::Function => {
                done.arguments = Some(item.arguments.clone());
                done.name = Some(item.name.clone());
            }
            ToolKind::Custom => done.input = Some(item.arguments.clone()),
        }
        Ok(vec![
            emit(done)?,
            self.emit(
                openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone,
                None,
                Some(Box::new(tool_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::Completed,
                ))),
                Some(item.index),
                None,
                None,
            )?,
        ])
    }

    pub(super) fn response(
        &self,
        status: openai::ResponseStatus,
    ) -> Result<openai::ResponseObject, TransformError> {
        let mut indexed = Vec::new();
        if let Some(item) = self.text.as_ref() {
            indexed.push((
                item.index,
                message_item(item, openai::ResponseItemLifecycleStatus::Completed),
            ));
        }
        if let Some(item) = self.reasoning.as_ref() {
            indexed.push((
                item.index,
                reasoning_item(item, openai::ResponseItemLifecycleStatus::Completed),
            ));
        }
        indexed.extend(self.tools.values().map(|item| {
            (
                item.index,
                tool_item(item, openai::ResponseItemLifecycleStatus::Completed),
            )
        }));
        indexed.sort_by_key(|(index, _)| *index);
        let output = indexed.into_iter().map(|(_, item)| item).collect();
        let incomplete_details = match self.finish_reason.as_ref() {
            Some(openai::ChatFinishReason::Length) => Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                rest: Default::default(),
            }),
            Some(openai::ChatFinishReason::ContentFilter) => Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                rest: Default::default(),
            }),
            _ => None,
        };
        Ok(openai::ResponseObject {
            id: self
                .id
                .clone()
                .ok_or_else(|| TransformError::shape("Chat stream", "id missing"))?,
            created_at: self.created_at,
            background: None,
            completed_at: None,
            conversation: None,
            error: None,
            incomplete_details,
            instructions: None,
            max_output_tokens: None,
            max_tool_calls: None,
            metadata: None,
            model: self.model.clone(),
            moderation: None,
            multi_agent: None,
            object: openai::ResponseObjectType::Response,
            output,
            output_text: self.text.as_ref().map(|item| item.text.clone()),
            parallel_tool_calls: None,
            prompt: None,
            prompt_cache_key: None,
            prompt_cache_options: None,
            prompt_cache_retention: None,
            previous_response_id: None,
            reasoning: None,
            safety_identifier: None,
            service_tier: self.service_tier.clone(),
            status: Some(status),
            store: None,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_logprobs: None,
            top_p: None,
            truncation: None,
            usage: self.usage.clone(),
            user: None,
            rest: self.response_rest.clone(),
        })
    }
}
