use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::generate_content::openai_responses_to_gemini_generate_content::usage;

use super::items::{item_id, required};
use super::{State, events};

impl State {
    pub(super) fn terminal(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let type_ = event.type_.clone();
        let response = required(event.response.map(|response| *response), "response")?;
        let expected_status = match type_ {
            openai::ResponseStreamEventTypeKnown::ResponseCompleted => {
                openai::ResponseStatus::Completed
            }
            openai::ResponseStreamEventTypeKnown::ResponseIncomplete => {
                openai::ResponseStatus::Incomplete
            }
            openai::ResponseStreamEventTypeKnown::ResponseFailed => openai::ResponseStatus::Failed,
            _ => {
                return Err(TransformError::shape(
                    "Responses stream",
                    "non-terminal event passed to terminal handler",
                ));
            }
        };
        if response
            .status
            .as_ref()
            .is_some_and(|status| status != &expected_status)
        {
            return Err(TransformError::shape(
                "Responses stream",
                "terminal event disagrees with response status",
            ));
        }
        let mut response_rest = response.rest.clone();
        if let Some(error) = response.error.as_ref() {
            response_rest.insert("openai_error".into(), serde_json::to_value(error)?);
        }
        self.remember(&response)?;
        let mut output = Vec::new();
        for (index, item) in response.output.iter().cloned().enumerate() {
            let key = item_id(&item).unwrap_or_else(|| format!("index:{index}"));
            output.extend(self.emit_item(item, key)?);
        }
        let finish = super::super::response::finish_reason(
            Some(&expected_status),
            response.incomplete_details.as_ref(),
        )?;
        let mut converted_usage = usage::to_gemini(response.usage)?;
        if let Some(usage) = converted_usage.as_mut() {
            usage.service_tier = super::config::openai_service_tier(response.service_tier);
        }
        let mut terminal = events::chunk(
            None,
            finish,
            converted_usage,
            self.response_id.clone(),
            self.model.clone(),
        );
        terminal.rest = response_rest;
        output.push(self.emit(terminal)?);
        self.stopped = true;
        Ok(output)
    }
}
