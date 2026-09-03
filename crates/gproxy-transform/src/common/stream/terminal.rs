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
        _extensions: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<Bytes>, TransformError> {
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
        Ok(match self.output {
            Output::Chat => {
                vec![self.chat_chunk(
                    empty_delta(),
                    Some(stop::claude_to_chat(&self.stop_reason)),
                    self.usage.clone().and_then(usage::claude_to_chat),
                )?]
            }
            Output::Responses => Vec::new(),
        })
    }

    pub(super) fn message_stop(
        &mut self,
        _extensions: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<Bytes>, TransformError> {
        if !self.started || !self.blocks.is_empty() {
            return Err(TransformError::shape(
                "Claude stream",
                "invalid message_stop",
            ));
        }
        self.stopped = true;
        Ok(match self.output {
            Output::Chat => {
                vec![SseFrame::encode(None, "[DONE]")]
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
                vec![self.response_terminal(incomplete, self.response_object(status))?]
            }
        })
    }
}
