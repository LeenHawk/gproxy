use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::usage;
use crate::models::common::wire_string;

use super::super::openai_to_claude::State;

impl State {
    pub(super) fn response_created(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.update_response(&event.response)?;
        self.ensure_start(event.response.rest.clone(), event.rest)
    }

    pub(super) fn response_pending(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.update_response(&event.response)?;
        let output = self.ensure_start(event.response.rest.clone(), event.rest.clone())?;
        self.pending_rest.extend(event.response.rest.clone());
        self.pending_rest.extend(event.rest);
        Ok(output)
    }

    pub(super) fn response_completed(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.update_response(&event.response)?;
        let mut output = self.ensure_start(event.response.rest.clone(), event.rest.clone())?;
        let mut terminal_rest = std::mem::take(&mut self.pending_rest);
        terminal_rest.extend(event.rest);
        let response = event.response;
        if self.next_index == 0 && !response.output.is_empty() {
            let converted =
                crate::generate_content::claude_messages_to_openai_responses::response::transform(
                    Bytes::from(serde_json::to_vec(response.as_ref())?),
                )?;
            let message: claude::CreateMessageResponseBody = serde_json::from_slice(&converted)?;
            for block in message.content {
                self.has_tool |= matches!(block, claude::ResponseContentBlock::ToolUse(_));
                let index = self.allocate();
                output.extend(self.block_start(index, block, Default::default())?);
                output.extend(self.close(index, Default::default())?);
            }
        }
        output.extend(self.finish_message(
            claude::StopReason::Known(if self.has_tool {
                claude::StopReasonKnown::ToolUse
            } else {
                claude::StopReasonKnown::EndTurn
            }),
            usage::responses_to_claude(response.usage),
            true,
            terminal_rest,
        )?);
        Ok(output)
    }

    pub(super) fn response_incomplete(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.update_response(&event.response)?;
        let mut output = self.ensure_start(event.response.rest.clone(), event.rest.clone())?;
        let mut terminal_rest = std::mem::take(&mut self.pending_rest);
        terminal_rest.extend(event.rest);
        let usage = usage::responses_to_claude(event.response.usage);
        output.extend(self.finish_message(
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens),
            usage,
            true,
            terminal_rest,
        )?);
        Ok(output)
    }

    fn update_response(&mut self, response: &openai::ResponseObject) -> Result<(), TransformError> {
        self.id = Some(response.id.clone());
        if let Some(model) = response.model.as_ref() {
            self.model = Some(wire_string(model)?.into());
        }
        Ok(())
    }
}
