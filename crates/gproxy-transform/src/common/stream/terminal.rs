use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};

use super::claude_to_chat::empty_delta;
use super::claude_to_openai::{Output, OutputEvent, State};
use super::state::merge_usage;

impl State {
    pub(super) fn message_delta(
        &mut self,
        delta: claude::MessageDelta,
        usage_delta: Option<claude::Usage>,
        _extensions: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<OutputEvent>, TransformError> {
        if let Some(reason) = delta.stop_reason {
            self.stop_reason = reason;
        }
        let refusal = stop::refusal_text(&self.stop_reason, delta.stop_details.as_ref());
        if let Some(usage) = usage_delta {
            if let Some(current) = self.usage.as_mut() {
                merge_usage(current, usage);
            } else {
                self.usage = Some(usage);
            }
        }
        Ok(match self.output {
            Output::Chat => {
                let mut delta = empty_delta();
                delta.refusal = refusal;
                vec![self.chat_chunk(
                    delta,
                    Some(stop::claude_to_chat(&self.stop_reason)),
                    self.usage.clone().and_then(usage::claude_to_chat),
                )?]
            }
            Output::Responses => {
                if let Some(refusal) = refusal {
                    let index = self.completed.len() as u32;
                    let item = stop::refusal_item(
                        format!(
                            "msg_{}_refusal",
                            self.id.as_deref().expect("started message")
                        ),
                        refusal,
                    );
                    let output = vec![
                        self.response_output_item_added(item.clone(), index)?,
                        self.response_output_item_done(item.clone(), index)?,
                    ];
                    self.completed.push(item);
                    output
                } else {
                    Vec::new()
                }
            }
        })
    }

    pub(super) fn message_stop(
        &mut self,
        _extensions: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Vec<OutputEvent>, TransformError> {
        if !self.started || !self.blocks.is_empty() {
            return Err(TransformError::shape(
                "Claude stream",
                "invalid message_stop",
            ));
        }
        self.stopped = true;
        Ok(match self.output {
            Output::Chat => Vec::new(),
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
