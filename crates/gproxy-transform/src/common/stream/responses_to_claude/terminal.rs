use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::usage;
use crate::models::common::wire_string;

use super::super::openai_to_claude::State;

impl State {
    pub(super) fn response_created(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        self.update_response(&event.response)?;
        self.ensure_start()
    }

    pub(super) fn response_pending(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        self.update_response(&event.response)?;
        let output = self.ensure_start()?;
        Ok(output)
    }

    pub(super) fn response_completed(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        self.update_response(&event.response)?;
        let mut output = self.ensure_start()?;
        let response = event.response;
        if self.next_index == 0 && !response.output.is_empty() {
            let message = crate::generate_content::claude_messages_to_openai_responses::response::transform_typed(
                response.as_ref().clone(),
            )?;
            for block in message.content {
                self.has_tool |= matches!(block, claude::ResponseContentBlock::ToolUse(_));
                let index = self.allocate();
                output.extend(self.block_start(index, block)?);
                output.extend(self.close(index)?);
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
        )?);
        Ok(output)
    }

    pub(super) fn response_incomplete(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        self.update_response(&event.response)?;
        let mut output = self.ensure_start()?;
        let usage = usage::responses_to_claude(event.response.usage);
        output.extend(self.finish_message(
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens),
            usage,
            true,
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
