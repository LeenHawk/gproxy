use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_responses::{config, usage};

use super::{Item, State, events};

impl State {
    pub(super) fn created(&mut self) -> Result<Bytes, TransformError> {
        let response = self.response(openai::ResponseStatus::InProgress, None, None, Vec::new())?;
        let sequence_number = Some(self.next_sequence());
        events::emit(openai::KnownResponseStreamEvent::ResponseCreated(
            openai::ResponseLifecycleEvent {
                response: Box::new(response),
                sequence_number,
                rest: Default::default(),
            },
        ))
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
        for (_, item) in std::mem::take(&mut self.text) {
            output.extend(self.finish_text(item)?);
        }
        for (_, item) in std::mem::take(&mut self.reasoning) {
            output.extend(self.finish_reasoning(item)?);
        }
        if self.audio {
            let sequence_number = Some(self.next_sequence());
            output.push(events::emit(
                openai::KnownResponseStreamEvent::ResponseAudioDone(
                    openai::ResponseSequenceEvent {
                        sequence_number,
                        rest: Default::default(),
                    },
                ),
            )?);
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
        let response = Box::new(openai::ResponseObject {
            service_tier,
            ..self.response(status.clone(), details, converted_usage, items)?
        });
        let sequence_number = Some(self.next_sequence());
        let event = match status {
            openai::ResponseStatus::Completed => {
                openai::KnownResponseStreamEvent::ResponseCompleted(
                    openai::ResponseLifecycleEvent {
                        response,
                        sequence_number,
                        rest: Default::default(),
                    },
                )
            }
            openai::ResponseStatus::Incomplete => {
                openai::KnownResponseStreamEvent::ResponseIncomplete(
                    openai::ResponseLifecycleEvent {
                        response,
                        sequence_number,
                        rest: Default::default(),
                    },
                )
            }
            openai::ResponseStatus::Failed => {
                openai::KnownResponseStreamEvent::ResponseFailed(openai::ResponseLifecycleEvent {
                    response,
                    sequence_number,
                    rest: Default::default(),
                })
            }
            openai::ResponseStatus::InProgress
            | openai::ResponseStatus::Cancelled
            | openai::ResponseStatus::Queued
            | openai::ResponseStatus::Unknown(_) => {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "unsupported terminal status",
                ));
            }
        };
        output.push(events::emit(event)?);
        self.stopped = true;
        Ok(output)
    }

    pub(super) fn finish_candidate(
        &mut self,
        candidate_index: i32,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if let Some(item) = self.text.remove(&candidate_index) {
            output.extend(self.finish_text(item)?);
        }
        if let Some(item) = self.reasoning.remove(&candidate_index) {
            output.extend(self.finish_reasoning(item)?);
        }
        Ok(output)
    }

    fn finish_text(&mut self, item: Item) -> Result<Vec<Bytes>, TransformError> {
        let text_done = openai::KnownResponseStreamEvent::ResponseOutputTextDone(
            openai::ResponseOutputTextDoneEvent {
                content_index: 0,
                item_id: item.id.clone(),
                logprobs: None,
                output_index: item.index,
                sequence_number: Some(self.next_sequence()),
                text: item.text.clone(),
                rest: Default::default(),
            },
        );
        let part_done = openai::KnownResponseStreamEvent::ResponseContentPartDone(
            openai::ResponseContentPartEvent {
                content_index: 0,
                item_id: item.id.clone(),
                output_index: item.index,
                part: openai::ResponseContentPart::OutputText(events::message_part(&item)),
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        );
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
            let text_done = openai::KnownResponseStreamEvent::ResponseReasoningTextDone(
                openai::ResponseContentTextDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    sequence_number: Some(self.next_sequence()),
                    text: item.text.clone(),
                    rest: Default::default(),
                },
            );
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
        events::emit(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
            openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        ))
    }
}
