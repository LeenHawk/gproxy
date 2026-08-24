use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};
use crate::envelope::SseFrame;

use super::claude_to_chat::empty_delta;
use super::claude_to_openai::{Output, State};
use super::state::merge_usage;

impl State {
    pub(super) fn message_delta(
        &mut self,
        delta: claude::MessageDelta,
        usage_delta: Option<claude::Usage>,
        rest: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut event_rest = delta.rest;
        event_rest.extend(rest);
        if let Some(reason) = delta.stop_reason {
            self.stop_reason = reason;
        }
        if let Some(usage) = usage_delta {
            if let Some(current) = self.usage.as_mut() {
                merge_usage(current, usage);
            } else {
                self.usage = Some(usage);
            }
        }
        self.response_rest.extend(event_rest);
        Ok(match self.output {
            Output::Chat => {
                let rest = std::mem::take(&mut self.response_rest);
                vec![self.chat_chunk(
                    empty_delta(rest),
                    Some(stop::claude_to_chat(&self.stop_reason)),
                    self.usage.clone().and_then(usage::claude_to_chat),
                )?]
            }
            Output::Responses => Vec::new(),
        })
    }

    pub(super) fn message_stop(
        &mut self,
        rest: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<Bytes>, TransformError> {
        if !self.started || !self.blocks.is_empty() {
            return Err(TransformError::shape(
                "Claude stream",
                "invalid message_stop",
            ));
        }
        self.response_rest.extend(rest);
        self.stopped = true;
        Ok(match self.output {
            Output::Chat => {
                let mut frames = Vec::new();
                if !self.response_rest.is_empty() {
                    let rest = std::mem::take(&mut self.response_rest);
                    frames.push(self.chat_chunk(empty_delta(rest), None, None)?);
                }
                frames.push(SseFrame::encode(None, "[DONE]"));
                frames
            }
            Output::Responses => {
                let incomplete = matches!(
                    self.stop_reason,
                    claude::StopReason::Known(
                        claude::StopReasonKnown::MaxTokens
                            | claude::StopReasonKnown::ModelContextWindowExceeded
                            | claude::StopReasonKnown::Refusal
                    )
                );
                let status = if incomplete {
                    openai::ResponseStatus::Incomplete
                } else {
                    openai::ResponseStatus::Completed
                };
                vec![self.response_terminal(
                    incomplete,
                    self.response_object(status),
                    Default::default(),
                )?]
            }
        })
    }
}
