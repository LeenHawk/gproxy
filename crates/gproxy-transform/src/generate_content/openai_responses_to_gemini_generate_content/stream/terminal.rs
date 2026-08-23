use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_responses::{config, usage};

use super::{Item, State, events};

impl State {
    pub(super) fn created(&mut self) -> Result<Bytes, TransformError> {
        let response = self.response(openai::ResponseStatus::InProgress, None, None, Vec::new())?;
        let mut event = events::event(openai::ResponseStreamEventTypeKnown::ResponseCreated);
        event.sequence_number = Some(self.next_sequence());
        event.response = Some(Box::new(response));
        events::emit(event)
    }

    pub(super) fn terminal(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            return Ok(Vec::new());
        }
        if !self.pending.is_empty() || !self.started {
            return Err(TransformError::IncompleteStream);
        }
        let (mut status, mut details) = super::super::response::response_status(&self.candidates);
        if status.is_none() && self.blocked {
            status = Some(openai::ResponseStatus::Incomplete);
            details = Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                rest: Default::default(),
            });
        }
        let status = status.ok_or(TransformError::IncompleteStream)?;
        let mut output = Vec::new();
        if let Some(item) = self.text.take() {
            output.extend(self.finish_text(item)?);
        }
        if let Some(item) = self.reasoning.take() {
            output.extend(self.finish_reasoning(item)?);
        }
        if self.audio {
            let mut event = events::event(openai::ResponseStreamEventTypeKnown::ResponseAudioDone);
            event.sequence_number = Some(self.next_sequence());
            output.push(events::emit(event)?);
        }
        self.items.sort_by_key(|(index, _)| *index);
        let items = self
            .items
            .iter()
            .map(|(_, item)| item.clone())
            .collect::<Vec<_>>();
        let service_tier = self
            .usage
            .as_ref()
            .and_then(|usage| config::gemini_service_tier(usage.service_tier.clone()));
        let converted_usage = self.usage.take().map(usage::to_responses).transpose()?;
        let response = self.response(status.clone(), details, converted_usage, items)?;
        let type_ = match status {
            openai::ResponseStatus::Completed => {
                openai::ResponseStreamEventTypeKnown::ResponseCompleted
            }
            openai::ResponseStatus::Incomplete => {
                openai::ResponseStreamEventTypeKnown::ResponseIncomplete
            }
            openai::ResponseStatus::Failed => openai::ResponseStreamEventTypeKnown::ResponseFailed,
            _ => {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "unsupported terminal status",
                ));
            }
        };
        let mut event = events::event(type_);
        event.sequence_number = Some(self.next_sequence());
        event.response = Some(Box::new(openai::ResponseObject {
            service_tier,
            ..response
        }));
        output.push(events::emit(event)?);
        self.stopped = true;
        Ok(output)
    }

    fn finish_text(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let mut text_done =
            events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputTextDone);
        text_done.sequence_number = Some(self.next_sequence());
        text_done.item_id = Some(item.id.clone());
        text_done.output_index = Some(item.index);
        text_done.content_index = Some(0);
        text_done.text = Some(item.text.clone());
        let mut part_done =
            events::event(openai::ResponseStreamEventTypeKnown::ResponseContentPartDone);
        part_done.sequence_number = Some(self.next_sequence());
        part_done.item_id = Some(item.id.clone());
        part_done.output_index = Some(item.index);
        part_done.content_index = Some(0);
        part_done.part = Some(openai::ResponseContentPart::OutputText(
            events::message_part(&item),
        ));
        let response_item =
            events::message_item(&item, openai::ResponseItemLifecycleStatus::Completed);
        let done = self.item_done(item.index, response_item.clone())?;
        self.items.push((item.index, response_item));
        Ok(vec![
            events::emit(text_done)?,
            events::emit(part_done)?,
            done,
        ])
    }

    fn finish_reasoning(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if !item.text.is_empty() {
            let mut text_done =
                events::event(openai::ResponseStreamEventTypeKnown::ResponseReasoningTextDone);
            text_done.sequence_number = Some(self.next_sequence());
            text_done.item_id = Some(item.id.clone());
            text_done.output_index = Some(item.index);
            text_done.content_index = Some(0);
            text_done.text = Some(item.text.clone());
            output.push(events::emit(text_done)?);
        }
        let response_item =
            events::reasoning_item(&item, openai::ResponseItemLifecycleStatus::Completed);
        output.push(self.item_done(item.index, response_item.clone())?);
        self.items.push((item.index, response_item));
        Ok(output)
    }

    fn item_done(
        &mut self,
        index: u32,
        item: openai::ResponseItem,
    ) -> Result<Bytes, TransformError> {
        let mut event = events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone);
        event.sequence_number = Some(self.next_sequence());
        event.item_id = Some(events::item_id(&item, index));
        event.output_index = Some(index);
        event.item = Some(Box::new(item));
        events::emit(event)
    }
}
