use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;
use crate::envelope::SseFrame;

use super::State;
use super::wire::empty_delta;

impl State {
    pub(super) fn start(
        &mut self,
        event: openai::ResponseLifecycleEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.update_response(&event.response);
        Ok(Vec::new())
    }

    pub(super) fn terminal(
        &mut self,
        event: openai::ResponseLifecycleEvent,
        expected: openai::ResponseStatus,
    ) -> Result<Vec<Bytes>, TransformError> {
        let response = event.response;
        self.update_response(&response);
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
            openai::ResponseStatus::Failed | openai::ResponseStatus::Cancelled => {
                openai::ChatFinishReason::ContentFilter
            }
            openai::ResponseStatus::InProgress | openai::ResponseStatus::Queued => {
                openai::ChatFinishReason::Stop
            }
            openai::ResponseStatus::Unknown(_) => openai::ChatFinishReason::ContentFilter,
        };
        self.stopped = true;
        let mut output = vec![self.chunk(
            empty_delta(),
            Some(finish),
            response.usage.clone().map(usage::responses_to_chat),
        )?];
        output.push(SseFrame::encode(None, "[DONE]"));
        Ok(output)
    }

    pub(super) fn error_terminal(&mut self) -> Result<Vec<Bytes>, TransformError> {
        self.id.get_or_insert_with(|| "resp_error".into());
        self.model
            .get_or_insert_with(|| openai::OpenAiModelId::from("unknown"));
        self.stopped = true;
        Ok(vec![
            self.chunk(
                empty_delta(),
                Some(openai::ChatFinishReason::ContentFilter),
                None,
            )?,
            SseFrame::encode(None, "[DONE]"),
        ])
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
                rest: Default::default(),
            },
        )
    }
}
