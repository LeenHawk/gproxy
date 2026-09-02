mod content;
mod repair;
mod state;

use std::collections::BTreeMap;

use gproxy_protocol::openai::generate_content::responses::{
    KnownResponseStreamEvent as Known, ResponseStreamEvent,
};

use self::repair::input_done_event;
use self::state::{InputKind, ItemState};
use super::event;

#[derive(Default)]
pub(super) struct Lifecycle {
    items: BTreeMap<u32, ItemState>,
    terminal: bool,
}

impl Lifecycle {
    pub(super) fn normalize(
        &mut self,
        mut event: ResponseStreamEvent,
    ) -> Result<Vec<ResponseStreamEvent>, String> {
        let ResponseStreamEvent::Known(known) = &mut event else {
            return Ok(vec![event]);
        };
        match known.as_mut() {
            Known::ResponseOutputItemAdded(payload) => self.note_item(payload, true),
            Known::ResponseOutputItemDone(payload) => {
                self.note_item(payload, false);
                let index = payload.output_index;
                let state = self.items.entry(index).or_default();
                let mut output = Vec::new();
                if let Some(item) = state.item.clone()
                    && !state.added
                {
                    output.push(event::output_item_added(
                        index,
                        item,
                        None,
                        Default::default(),
                    ));
                    state.added = true;
                }
                if state.input_kind.is_some() && !state.input_done {
                    if let Some(done) = input_done_event(index, state)? {
                        output.push(done);
                    }
                    state.input_done = true;
                }
                state.done = true;
                output.push(event);
                return Ok(output);
            }
            Known::ResponseOutputTextDelta(payload) => {
                self.push_text(
                    payload.output_index,
                    &payload.item_id,
                    &payload.delta,
                    false,
                );
                if let Some(added) = self.added_event(payload.output_index) {
                    return Ok(vec![added, event]);
                }
            }
            Known::ResponseReasoningTextDelta(payload) => {
                self.push_text(payload.output_index, &payload.item_id, &payload.delta, true);
                if let Some(added) = self.added_event(payload.output_index) {
                    return Ok(vec![added, event]);
                }
            }
            Known::ResponseFunctionCallArgumentsDelta(payload) => self.push_input(
                payload.output_index,
                InputKind::Function,
                Some(&payload.item_id),
                None,
                &payload.delta,
                false,
            ),
            Known::ResponseFunctionCallArgumentsDone(payload) => self.push_input(
                payload.output_index,
                InputKind::Function,
                payload.item_id.as_deref(),
                payload.name.as_deref(),
                &payload.arguments,
                true,
            ),
            Known::ResponseCustomToolCallInputDelta(payload) => self.push_input(
                payload.output_index,
                InputKind::Custom,
                Some(&payload.item_id),
                None,
                &payload.delta,
                false,
            ),
            Known::ResponseCustomToolCallInputDone(payload) => self.push_input(
                payload.output_index,
                InputKind::Custom,
                Some(&payload.item_id),
                None,
                &payload.input,
                true,
            ),
            Known::ResponseCompleted(payload)
            | Known::ResponseIncomplete(payload)
            | Known::ResponseFailed(payload) => {
                self.terminal = true;
                let mut output = self.repair()?;
                if payload.response.output.is_empty() {
                    payload.response.output = self.completed_items();
                }
                output.push(event);
                return Ok(output);
            }
            Known::ResponseCreated(_)
            | Known::ResponseInProgress(_)
            | Known::ResponseQueued(_)
            | Known::ResponseContentPartAdded(_)
            | Known::ResponseContentPartDone(_)
            | Known::ResponseOutputTextDone(_)
            | Known::ResponseOutputTextAnnotationAdded(_)
            | Known::ResponseRefusalDelta(_)
            | Known::ResponseRefusalDone(_)
            | Known::ResponseReasoningSummaryPartAdded(_)
            | Known::ResponseReasoningSummaryPartDone(_)
            | Known::ResponseReasoningSummaryTextDelta(_)
            | Known::ResponseReasoningSummaryTextDone(_)
            | Known::ResponseReasoningTextDone(_)
            | Known::ResponseAudioDelta(_)
            | Known::ResponseAudioDone(_)
            | Known::ResponseAudioTranscriptDelta(_)
            | Known::ResponseAudioTranscriptDone(_)
            | Known::ResponseImageGenerationCallCompleted(_)
            | Known::ResponseImageGenerationCallGenerating(_)
            | Known::ResponseImageGenerationCallInProgress(_)
            | Known::ResponseImageGenerationCallPartialImage(_)
            | Known::ResponseFileSearchCallInProgress(_)
            | Known::ResponseFileSearchCallSearching(_)
            | Known::ResponseFileSearchCallCompleted(_)
            | Known::ResponseWebSearchCallInProgress(_)
            | Known::ResponseWebSearchCallSearching(_)
            | Known::ResponseWebSearchCallCompleted(_)
            | Known::ResponseCodeInterpreterCallInProgress(_)
            | Known::ResponseCodeInterpreterCallInterpreting(_)
            | Known::ResponseCodeInterpreterCallCompleted(_)
            | Known::ResponseCodeInterpreterCallCodeDelta(_)
            | Known::ResponseCodeInterpreterCallCodeDone(_)
            | Known::ResponseMcpCallArgumentsDelta(_)
            | Known::ResponseMcpCallArgumentsDone(_)
            | Known::ResponseMcpCallInProgress(_)
            | Known::ResponseMcpCallCompleted(_)
            | Known::ResponseMcpCallFailed(_)
            | Known::ResponseMcpListToolsInProgress(_)
            | Known::ResponseMcpListToolsCompleted(_)
            | Known::ResponseMcpListToolsFailed(_)
            | Known::ResponseInjectCreated(_)
            | Known::ResponseInjectFailed(_)
            | Known::Error(_) => {}
        }
        Ok(vec![event])
    }

    pub(super) fn is_terminal(&self) -> bool {
        self.terminal
    }
}
