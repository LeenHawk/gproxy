use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;
use crate::envelope::SseFrame;

use super::State;
use super::wire::{empty_delta, merge_rest};

impl State {
    pub(super) fn start(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        if event
            .response
            .status
            .as_ref()
            .is_some_and(|status| status != &openai::ResponseStatus::InProgress)
        {
            return Err(TransformError::shape(
                "Responses stream",
                "start event response status is not in_progress",
            ));
        }
        self.update_response(&event.response);
        let mut rest = event.response.rest.clone();
        merge_rest(&mut rest, event.rest);
        if self.started {
            return if rest.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![self.chunk(empty_delta(), None, None, rest)?])
            };
        }
        self.started = true;
        Ok(vec![self.chunk(
            openai::ChatDelta {
                role: Some(openai::ChatDeltaRole::Assistant),
                ..empty_delta()
            },
            None,
            None,
            rest,
        )?])
    }

    pub(super) fn terminal(
        &mut self,
        event: openai::ResponseLifecycleEvent,
        expected: openai::ResponseStatus,
    ) -> Result<Vec<Bytes>, TransformError> {
        let response = event.response;
        if response
            .status
            .as_ref()
            .is_some_and(|status| status != &expected)
        {
            return Err(TransformError::shape(
                "Responses stream",
                "terminal event type does not match response status",
            ));
        }
        self.update_response(&response);
        let mut output = Vec::new();
        for (index, item) in response.output.iter().cloned().enumerate() {
            output.extend(self.complete_item(item, index as u32, Default::default())?);
        }
        let finish = match expected {
            openai::ResponseStatus::Completed if self.tools.is_empty() => {
                openai::ChatFinishReason::Stop
            }
            openai::ResponseStatus::Completed => openai::ChatFinishReason::ToolCalls,
            openai::ResponseStatus::Incomplete
                if matches!(
                    response
                        .incomplete_details
                        .as_ref()
                        .and_then(|value| value.reason.as_ref()),
                    Some(openai::IncompleteReason::ContentFilter)
                ) =>
            {
                openai::ChatFinishReason::ContentFilter
            }
            openai::ResponseStatus::Incomplete => openai::ChatFinishReason::Length,
            _ => {
                return Err(TransformError::shape(
                    "Responses stream",
                    "unsupported successful terminal status",
                ));
            }
        };
        let mut rest = response.rest.clone();
        merge_rest(&mut rest, event.rest);
        self.stopped = true;
        output.push(self.chunk(
            empty_delta(),
            Some(finish),
            response.usage.clone().map(usage::responses_to_chat),
            rest,
        )?);
        output.push(SseFrame::encode(None, "[DONE]"));
        Ok(output)
    }

    fn update_response(&mut self, response: &openai::ResponseObject) {
        self.id = Some(response.id.clone());
        self.created_at = response.created_at.or(self.created_at);
        self.model = response.model.clone().or(self.model.take());
        self.service_tier = response.service_tier.clone().or(self.service_tier.take());
    }

    pub(super) fn chunk(
        &self,
        delta: openai::ChatDelta,
        finish_reason: Option<openai::ChatFinishReason>,
        usage: Option<openai::CompletionUsage>,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        SseFrame::typed(
            None,
            &openai::ChatCompletionChunk {
                id: self
                    .id
                    .clone()
                    .ok_or_else(|| TransformError::shape("Responses stream", "id missing"))?,
                choices: vec![openai::ChatChunkChoice {
                    index: 0,
                    delta,
                    finish_reason,
                    logprobs: None,
                    rest: Default::default(),
                }],
                created: self.created_at,
                model: self
                    .model
                    .clone()
                    .ok_or_else(|| TransformError::shape("Responses stream", "model missing"))?,
                object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
                service_tier: self.service_tier.clone(),
                system_fingerprint: None,
                usage,
                rest,
            },
        )
    }
}
