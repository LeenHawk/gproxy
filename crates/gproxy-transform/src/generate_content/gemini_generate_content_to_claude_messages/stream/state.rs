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
pub(super) struct State {
    pub(super) correlation: Correlation,
    pub(super) next_index: u64,
    pub(super) open: Option<OpenBlock>,
    started: bool,
    pub(super) has_tool: bool,
    saw_finish: bool,
    stopped: bool,
}

impl State {
    fn chunk(
        &mut self,
        mut chunk: gemini::GenerateContentResponse,
    ) -> Result<Vec<Bytes>, TransformError> {
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
            output.push(events::encode(events::start(
                id,
                model,
                std::mem::take(&mut chunk.rest),
            ))?);
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
                output.push(events::encode(events::message_delta(
                    Some(response::finish_reason(reason, self.has_tool)?),
                    candidate.finish_message,
                    usage,
                    candidate.rest,
                ))?);
                self.saw_finish = true;
            } else if usage.is_some() {
                output.push(events::encode(events::message_delta(
                    None,
                    None,
                    usage,
                    candidate.rest,
                ))?);
            }
        } else if chunk
            .prompt_feedback
            .map(response::blocked)
            .transpose()?
            .unwrap_or(false)
        {
            output.extend(self.close_open()?);
            output.push(events::encode(events::message_delta(
                Some(claude::StopReason::Known(claude::StopReasonKnown::Refusal)),
                None,
                usage,
                chunk.rest,
            ))?);
            self.saw_finish = true;
        } else if usage.is_some() {
            output.push(events::encode(events::message_delta(
                None, None, usage, chunk.rest,
            ))?);
        }
        Ok(output)
    }

    pub(super) fn next_block(&mut self, kind: OpenKind) -> u64 {
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        self.open = Some(OpenBlock { index, kind });
        index
    }

    pub(super) fn close_open(&mut self) -> Result<Vec<Bytes>, TransformError> {
        self.open
            .take()
            .map(|open| events::encode(events::block_stop(open.index)).map(|event| vec![event]))
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.chunk(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if !self.started || !self.saw_finish || self.stopped || self.open.is_some() {
            return Err(TransformError::IncompleteStream);
        }
        self.stopped = true;
        Ok(vec![events::encode(events::message_stop())?])
    }
}
