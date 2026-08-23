use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{Item, State, events};

impl State {
    pub(super) fn chunk(
        &mut self,
        chunk: gemini::GenerateContentResponse,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.response_rest.extend(chunk.rest);
        self.blocked |= chunk
            .prompt_feedback
            .as_ref()
            .and_then(|feedback| feedback.block_reason.as_ref())
            .is_some();
        if let Some(usage) = chunk.usage_metadata {
            self.usage = Some(usage);
        }
        if chunk.candidates.len() > 1 {
            return Err(TransformError::unsupported(
                "Gemini stream",
                "multiple candidates",
            ));
        }
        let mut output = Vec::new();
        for mut candidate in chunk.candidates {
            if candidate.index.is_some_and(|index| index != 0) {
                return Err(TransformError::unsupported(
                    "Gemini stream",
                    "multiple candidates",
                ));
            }
            if self.finished_candidate
                && (candidate.content.is_some() || candidate.finish_reason.is_some())
            {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "content or a second finish received after candidate finish",
                ));
            }
            if let Some(content) = candidate.content.take() {
                if content.role.as_ref().is_some_and(|role| {
                    !matches!(
                        role,
                        gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)
                    )
                }) {
                    return Err(TransformError::unsupported(
                        "Gemini stream response",
                        "non-model content role",
                    ));
                }
                for part in content.parts {
                    output.extend(self.part(part)?);
                }
            }
            if candidate.finish_reason.is_some() {
                self.finished_candidate = true;
                self.candidates.push(candidate);
            }
        }
        Ok(output)
    }

    pub(super) fn text_delta(
        &mut self,
        text: String,
        thought: bool,
        signature: Option<String>,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if thought {
            if self.reasoning.is_none() {
                let item = Item {
                    id: format!("rs_{}", required_id(self)?),
                    index: self.allocate(),
                    text: String::new(),
                    signature: None,
                    rest,
                };
                output.push(self.item_added(events::reasoning_item(
                    &item,
                    openai::ResponseItemLifecycleStatus::InProgress,
                ))?);
                self.reasoning = Some(item);
            }
            let (item_id, item_index) = {
                let item = self.reasoning.as_mut().expect("created above");
                item.text.push_str(&text);
                if signature.is_some() {
                    item.signature = signature;
                }
                (item.id.clone(), item.index)
            };
            if !text.is_empty() {
                let mut event =
                    events::event(openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDelta);
                event.sequence_number = Some(self.next_sequence());
                event.item_id = Some(item_id);
                event.output_index = Some(item_index);
                event.content_index = Some(0);
                event.delta = Some(text);
                output.push(events::emit(event)?);
            }
            return Ok(output);
        }
        if self.text.is_none() {
            let item = Item {
                id: format!("msg_{}", required_id(self)?),
                index: self.allocate(),
                text: String::new(),
                signature: None,
                rest,
            };
            output.push(self.item_added(events::message_item(
                &item,
                openai::ResponseItemLifecycleStatus::InProgress,
            ))?);
            let mut added =
                events::event(openai::ResponseStreamEventTypeKnown::ResponseContentPartAdded);
            added.sequence_number = Some(self.next_sequence());
            added.item_id = Some(item.id.clone());
            added.output_index = Some(item.index);
            added.content_index = Some(0);
            added.part = Some(openai::ResponseContentPart::OutputText(
                events::message_part(&item),
            ));
            output.push(events::emit(added)?);
            self.text = Some(item);
        }
        let (item_id, item_index) = {
            let item = self.text.as_mut().expect("created above");
            item.text.push_str(&text);
            (item.id.clone(), item.index)
        };
        if !text.is_empty() {
            let mut event =
                events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputTextDelta);
            event.sequence_number = Some(self.next_sequence());
            event.item_id = Some(item_id);
            event.output_index = Some(item_index);
            event.content_index = Some(0);
            event.delta = Some(text);
            output.push(events::emit(event)?);
        }
        Ok(output)
    }

    fn item_added(&mut self, item: openai::ResponseItem) -> Result<Bytes, TransformError> {
        let index = match &item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                if message
                    .id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("msg_"))
                {
                    self.text.as_ref().map(|item| item.index)
                } else {
                    self.reasoning.as_ref().map(|item| item.index)
                }
            }
            _ => None,
        }
        .unwrap_or(self.next_index.saturating_sub(1));
        let mut event =
            events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded);
        event.sequence_number = Some(self.next_sequence());
        event.item = Some(Box::new(item));
        event.output_index = Some(index);
        events::emit(event)
    }
}

fn required_id(state: &State) -> Result<&str, TransformError> {
    state
        .id
        .as_deref()
        .ok_or_else(|| TransformError::shape("Gemini stream", "responseId missing"))
}
