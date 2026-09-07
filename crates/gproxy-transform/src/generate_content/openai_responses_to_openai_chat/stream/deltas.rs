use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, stream_logprob};
use super::{Item, Scalar, State, content};

impl State {
    pub(super) fn scalar_delta(
        &mut self,
        kind: Scalar,
        delta: String,
        logprobs: Vec<openai::TokenLogprob>,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        if delta.is_empty() && logprobs.is_empty() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        if self.scalar.as_ref().is_some_and(|item| item.kind != kind) {
            output.extend(self.finish_scalar()?);
        }
        if self.scalar.is_none() {
            let index = self.allocate();
            let prefix = if kind == Scalar::Reasoning {
                "rs"
            } else {
                "msg"
            };
            let item = Item {
                kind,
                id: format!("{}_{index}", self.item_id(prefix)?),
                index,
                text: String::new(),
                logprobs: Vec::new(),
            };
            output.push(self.item_added(
                index,
                content::item(&item, openai::ResponseItemLifecycleStatus::InProgress),
            )?);
            if kind != Scalar::Reasoning {
                output.push(emit(
                    openai::KnownResponseStreamEvent::ResponseContentPartAdded(
                        self.part_event(&item),
                    ),
                )?);
            }
            self.scalar = Some(item);
        }
        let item = self.scalar.as_mut().expect("created");
        item.text.push_str(&delta);
        item.logprobs.extend(logprobs.clone());
        let item_id = item.id.clone();
        let output_index = item.index;
        let sequence_number = Some(self.next_sequence());
        let event = match kind {
            Scalar::Text => openai::KnownResponseStreamEvent::ResponseOutputTextDelta(
                crate::wire!(openai::ResponseOutputTextDeltaEvent {
                    content_index: Some(0),
                    delta,
                    item_id,
                    logprobs: (!logprobs.is_empty())
                        .then(|| logprobs.into_iter().map(stream_logprob).collect()),
                    output_index,
                    sequence_number,
                    rest: Default::default(),
                }),
            ),
            Scalar::Reasoning | Scalar::Refusal => {
                let payload = crate::wire!(openai::ResponseContentDeltaEvent {
                    content_index: 0,
                    delta,
                    item_id,
                    output_index,
                    sequence_number,
                    rest: Default::default(),
                });
                if kind == Scalar::Reasoning {
                    openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(payload)
                } else {
                    openai::KnownResponseStreamEvent::ResponseRefusalDelta(payload)
                }
            }
        };
        output.push(emit(event)?);
        Ok(output)
    }
}
