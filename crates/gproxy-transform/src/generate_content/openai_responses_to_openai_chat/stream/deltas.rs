use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, stream_logprob};
use super::tools::merge_rest;
use super::{Item, State};

impl State {
    pub(super) fn text_delta(
        &mut self,
        delta: String,
        rest: openai::Rest,
        logprobs: Vec<openai::TokenLogprob>,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.text.is_none() {
            let item = Item {
                id: self.item_id("msg")?,
                index: self.allocate(),
                text: String::new(),
                rest: rest.clone(),
                logprobs: Vec::new(),
            };
            self.text = Some(item);
        }
        let (id, index) = {
            let item = self.text.as_mut().expect("created");
            item.text.push_str(&delta);
            item.logprobs.extend(logprobs.clone());
            merge_rest(&mut item.rest, rest);
            (item.id.clone(), item.index)
        };
        Ok(vec![self.emit_text_delta(id, index, delta, logprobs)?])
    }

    pub(super) fn reasoning_delta(
        &mut self,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.reasoning.is_none() {
            let item = Item {
                id: self.item_id("rs")?,
                index: self.allocate(),
                text: String::new(),
                rest: rest.clone(),
                logprobs: Vec::new(),
            };
            self.reasoning = Some(item);
        }
        let (id, index) = {
            let item = self.reasoning.as_mut().expect("created");
            item.text.push_str(&delta);
            merge_rest(&mut item.rest, rest);
            (item.id.clone(), item.index)
        };
        Ok(vec![emit(
            openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(
                openai::ResponseContentDeltaEvent {
                    content_index: 0,
                    delta,
                    item_id: id,
                    output_index: index,
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                },
            ),
        )?])
    }

    pub(super) fn refusal_delta(
        &mut self,
        output_index: u32,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        Ok(vec![emit(
            openai::KnownResponseStreamEvent::ResponseRefusalDelta(
                openai::ResponseContentDeltaEvent {
                    content_index: 0,
                    delta,
                    item_id: format!("msg_{output_index}"),
                    output_index,
                    sequence_number: Some(self.next_sequence()),
                    rest,
                },
            ),
        )?])
    }

    fn emit_text_delta(
        &mut self,
        item_id: String,
        output_index: u32,
        delta: String,
        logprobs: Vec<openai::TokenLogprob>,
    ) -> Result<Bytes, TransformError> {
        let logprobs =
            (!logprobs.is_empty()).then(|| logprobs.into_iter().map(stream_logprob).collect());
        emit(openai::KnownResponseStreamEvent::ResponseOutputTextDelta(
            openai::ResponseOutputTextDeltaEvent {
                content_index: Some(0),
                delta,
                item_id,
                logprobs,
                output_index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        ))
    }
}
