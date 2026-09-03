use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

use super::State;

impl State {
    pub(crate) fn push_typed(
        &mut self,
        chunk: openai::ChatCompletionChunk,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        self.id = Some(chunk.id);
        self.created_at = chunk.created.or(self.created_at);
        self.model = Some(chunk.model);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        self.usage = chunk
            .usage
            .map(usage::chat_to_responses)
            .or(self.usage.take());
        let mut output = Vec::new();
        for choice in chunk.choices {
            let terminal = choice.finish_reason.is_some();
            output.extend(self.choice(choice)?);
            if terminal {
                output.extend(self.stop()?);
            }
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
            output.extend(self.text_delta(text, content_logprobs)?);
        } else if !content_logprobs.is_empty() {
            return Err(TransformError::shape(
                "Chat stream",
                "content logprobs without content delta",
            ));
        }
        if let Some(reasoning) = delta.reasoning_content {
            output.extend(self.reasoning_delta(reasoning)?);
        }
        if let Some(refusal) = delta.refusal {
            output.extend(self.refusal_delta(choice.index, refusal)?);
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
        }
        Ok(output)
    }
}
