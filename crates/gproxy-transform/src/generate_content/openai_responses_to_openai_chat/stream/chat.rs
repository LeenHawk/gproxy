use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

use super::events::emit;
use super::{Scalar, State};

impl State {
    pub(crate) fn push_typed(
        &mut self,
        chunk: openai::ChatCompletionChunk,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        if self.stopped {
            return Err(TransformError::shape("Chat stream", "chunk after finish"));
        }
        self.id = Some(chunk.id);
        self.created_at = chunk.created.or(self.created_at);
        self.model = Some(chunk.model);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        self.usage = chunk
            .usage
            .map(usage::chat_to_responses)
            .or(self.usage.take());
        let mut output = Vec::new();
        if !self.started {
            let response = self.response(openai::ResponseStatus::InProgress)?;
            output.push(emit(openai::KnownResponseStreamEvent::ResponseCreated(
                crate::wire!(openai::ResponseLifecycleEvent {
                    response: Box::new(response),
                    sequence_number: Some(self.next_sequence()),
                    rest: Default::default(),
                }),
            ))?);
            self.started = true;
        }
        for choice in chunk.choices {
            if choice.index != 0 {
                return Err(TransformError::unsupported(
                    "Chat stream",
                    "multiple choices",
                ));
            }
            if self.finish_reason.is_some() {
                return Err(TransformError::shape(
                    "Chat stream",
                    "choice after finish_reason",
                ));
            }
            output.extend(self.choice(choice)?);
        }
        Ok(output)
    }

    pub(crate) fn finish_typed(
        &mut self,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        self.stop()
    }

    fn choice(
        &mut self,
        choice: openai::ChatChunkChoice,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let content_logprobs = choice
            .logprobs
            .map(|logprobs| logprobs.content)
            .unwrap_or_default();
        let delta = choice.delta;
        let mut output = Vec::new();
        if let Some(text) = delta.content {
            output.extend(self.scalar_delta(Scalar::Text, text, content_logprobs)?);
        } else if !content_logprobs.is_empty() {
            return Err(TransformError::shape(
                "Chat stream",
                "content logprobs without content delta",
            ));
        }
        if let Some(reasoning) = delta.reasoning_content {
            output.extend(self.scalar_delta(Scalar::Reasoning, reasoning, Vec::new())?);
        }
        if let Some(refusal) = delta.refusal {
            output.extend(self.scalar_delta(Scalar::Refusal, refusal, Vec::new())?);
        }
        for call in delta.tool_calls.into_iter().flatten() {
            output.extend(self.tool_delta(call)?);
        }
        if let Some(function) = delta.function_call {
            output.extend(self.tool_delta(crate::wire!(openai::ChatToolCallDelta {
                index: choice.index,
                id: Some(format!("call_{}", choice.index)),
                type_: Some(openai::ChatToolCallType::Function),
                function: Some(function),
                custom: None,
                rest: Default::default(),
            }))?);
        }
        if choice.finish_reason.is_some() {
            self.finish_reason = choice.finish_reason;
            output.extend(self.finish_items()?);
        }
        Ok(output)
    }
}
