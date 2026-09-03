use bytes::Bytes;
use gproxy_protocol::{claude, gemini};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

use super::super::content::Correlation;
use super::super::{response, usage};
use super::events;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum OpenKind {
    Text,
    Thinking,
}

pub(super) struct OpenBlock {
    pub(super) index: u64,
    pub(super) kind: OpenKind,
}

#[derive(Default)]
pub(crate) struct State {
    pub(super) correlation: Correlation,
    pub(super) next_index: u64,
    pub(super) open: Option<OpenBlock>,
    pub(super) pending_signature: Option<String>,
    started: bool,
    pub(super) has_tool: bool,
    saw_finish: bool,
    stopped: bool,
}

impl State {
    pub(crate) fn push_typed(
        &mut self,
        mut chunk: gemini::GenerateContentResponse,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        if self.stopped || self.saw_finish {
            return Err(TransformError::shape(
                "Gemini stream",
                "chunk after terminal finish",
            ));
        }
        let mut output = Vec::new();
        if !self.started {
            let id = chunk.response_id.take().ok_or_else(|| {
                TransformError::shape("Gemini stream", "responseId is missing on first chunk")
            })?;
            let model = chunk.model_version.take().ok_or_else(|| {
                TransformError::shape("Gemini stream", "modelVersion is missing on first chunk")
            })?;
            output.push(events::wrap(events::start(id, model, Default::default())));
            self.started = true;
        }
        if chunk.candidates.len() > 1 {
            return Err(TransformError::unsupported(
                "Gemini stream",
                "multiple candidates",
            ));
        }
        let usage = chunk.usage_metadata.map(usage::convert).transpose()?;
        if let Some(candidate) = chunk.candidates.into_iter().next() {
            if candidate.index.is_some_and(|index| index != 0) {
                return Err(TransformError::unsupported(
                    "Gemini stream",
                    "nonzero candidate index",
                ));
            }
            if let Some(content) = candidate.content {
                for part in content.parts {
                    output.extend(self.part(part)?);
                }
            }
            if let Some(reason) = candidate.finish_reason {
                output.extend(self.close_open()?);
                output.push(events::wrap(events::message_delta(
                    Some(response::finish_reason(reason, self.has_tool)?),
                    candidate.finish_message,
                    usage,
                    Default::default(),
                )));
                self.saw_finish = true;
            } else if usage.is_some() {
                output.push(events::wrap(events::message_delta(
                    None,
                    None,
                    usage,
                    Default::default(),
                )));
            }
        } else if chunk
            .prompt_feedback
            .map(response::blocked)
            .transpose()?
            .unwrap_or(false)
        {
            output.extend(self.close_open()?);
            output.push(events::wrap(events::message_delta(
                Some(claude::StopReason::Known(claude::StopReasonKnown::Refusal)),
                None,
                usage,
                Default::default(),
            )));
            self.saw_finish = true;
        } else if usage.is_some() {
            output.push(events::wrap(events::message_delta(
                None,
                None,
                usage,
                Default::default(),
            )));
        }
        Ok(output)
    }

    pub(super) fn next_block(&mut self, kind: OpenKind) -> u64 {
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        self.open = Some(OpenBlock { index, kind });
        index
    }

    pub(super) fn close_open(&mut self) -> Result<Vec<claude::StreamEvent>, TransformError> {
        Ok(self
            .open
            .take()
            .map(|open| vec![events::wrap(events::block_stop(open.index))])
            .unwrap_or_default())
    }

    pub(crate) fn finish_typed(&mut self) -> Result<Vec<claude::StreamEvent>, TransformError> {
        if !self.started || !self.saw_finish || self.stopped || self.open.is_some() {
            return Err(TransformError::IncompleteStream);
        }
        self.stopped = true;
        Ok(vec![events::wrap(events::message_stop())])
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.push_typed(serde_json::from_str(&frame.data)?)?
            .into_iter()
            .map(|event| SseFrame::typed(event.event_name(), &event))
            .collect()
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        self.finish_typed()?
            .into_iter()
            .map(|event| SseFrame::typed(event.event_name(), &event))
            .collect()
    }
}
