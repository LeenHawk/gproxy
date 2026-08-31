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
        let mut output = Vec::new();
        let mut finished_here = false;
        for (position, mut candidate) in chunk.candidates.into_iter().enumerate() {
            let candidate_index = candidate.index.unwrap_or(position as i32);
            self.seen_candidates.insert(candidate_index);
            if self.finished_candidates.contains(&candidate_index)
                && (candidate.content.is_some() || candidate.finish_reason.is_some())
            {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "content or a second finish received after candidate finish",
                ));
            }
            if let Some(content) = candidate.content.take() {
                for part in content.parts {
                    output.extend(self.part(candidate_index, part)?);
                }
            }
            if candidate.finish_reason.is_some() {
                self.finished_candidates.insert(candidate_index);
                output.extend(self.finish_candidate(candidate_index)?);
                self.candidates.push(candidate);
                finished_here = true;
            }
        }
        if finished_here && self.seen_candidates == self.finished_candidates {
            output.extend(self.terminal()?);
        }
        Ok(output)
    }

    pub(super) fn text_delta(
        &mut self,
        candidate_index: i32,
        text: String,
        thought: bool,
        signature: Option<String>,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if thought {
            if !self.reasoning.contains_key(&candidate_index) {
                let index = self.allocate();
                let item = Item {
                    id: format!("rs_{index}"),
                    index,
                    text: String::new(),
                    signature: None,
                    rest,
                };
                output.push(self.item_added(
                    index,
                    events::reasoning_item(&item, openai::ResponseItemLifecycleStatus::InProgress),
                )?);
                self.reasoning.insert(candidate_index, item);
            }
            let (item_id, item_index) = {
                let item = self
                    .reasoning
                    .get_mut(&candidate_index)
                    .expect("created above");
                item.text.push_str(&text);
                if signature.is_some() {
                    item.signature = signature;
                }
                (item.id.clone(), item.index)
            };
            if !text.is_empty() {
                let event = openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(
                    openai::ResponseContentDeltaEvent {
                        content_index: 0,
                        delta: text,
                        item_id,
                        output_index: item_index,
                        sequence_number: Some(self.next_sequence()),
                        rest: Default::default(),
                    },
                );
                output.push(events::emit(event)?);
            }
            return Ok(output);
        }
        if !self.text.contains_key(&candidate_index) {
            let index = self.allocate();
            let item = Item {
                id: format!("msg_{index}"),
                index,
                text: String::new(),
                signature: None,
                rest,
            };
            output.push(self.item_added(
                index,
                events::message_item(&item, openai::ResponseItemLifecycleStatus::InProgress),
            )?);
            let added = openai::KnownResponseStreamEvent::ResponseContentPartAdded(
                openai::ResponseContentPartEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    part: openai::ResponseContentPart::OutputText(events::message_part(&item)),
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            );
            output.push(events::emit(added)?);
            self.text.insert(candidate_index, item);
        }
        let (item_id, item_index) = {
            let item = self.text.get_mut(&candidate_index).expect("created above");
            item.text.push_str(&text);
            (item.id.clone(), item.index)
        };
        if !text.is_empty() {
            let event = openai::KnownResponseStreamEvent::ResponseOutputTextDelta(
                openai::ResponseOutputTextDeltaEvent {
                    content_index: Some(0),
                    delta: text,
                    item_id,
                    logprobs: None,
                    output_index: item_index,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            );
            output.push(events::emit(event)?);
        }
        Ok(output)
    }

    fn item_added(
        &mut self,
        index: u32,
        item: openai::ResponseItem,
    ) -> Result<Bytes, TransformError> {
        let event = openai::KnownResponseStreamEvent::ResponseOutputItemAdded(
            openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        );
        events::emit(event)
    }
}
