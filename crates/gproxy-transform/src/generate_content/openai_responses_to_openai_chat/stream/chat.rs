use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;
use crate::envelope::SseFrame;

use super::State;
use super::tools::merge_rest;

impl State {
    pub(super) fn chat(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if frame.data == "[DONE]" {
            return self.stop();
        }
        let chunk: openai::ChatCompletionChunk = serde_json::from_str(&frame.data)?;
        self.id = Some(chunk.id);
        self.created_at = chunk.created.or(self.created_at);
        self.model = Some(chunk.model);
        self.service_tier = chunk.service_tier.or(self.service_tier.take());
        merge_rest(&mut self.response_rest, chunk.rest);
        if let Some(fingerprint) = chunk.system_fingerprint {
            self.response_rest.insert(
                "system_fingerprint".into(),
                serde_json::Value::String(fingerprint),
            );
        }
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

    fn choice(&mut self, choice: openai::ChatChunkChoice) -> Result<Vec<Bytes>, TransformError> {
        merge_rest(&mut self.response_rest, choice.rest);
        let content_logprobs = choice
            .logprobs
            .map(|logprobs| logprobs.content)
            .unwrap_or_default();
        let delta = choice.delta;
        let has_content = delta.content.is_some();
        let has_reasoning = delta.reasoning_content.is_some();
        let has_refusal = delta.refusal.is_some();
        let has_tools = delta.tool_calls.is_some() || delta.function_call.is_some();
        let mut delta_rest = delta.rest;
        if let Some(obfuscation) = delta.obfuscation {
            delta_rest.insert("obfuscation".into(), serde_json::Value::String(obfuscation));
        }
        let mut output = Vec::new();
        if let Some(text) = delta.content {
            output.extend(self.text_delta(text, delta_rest.clone(), content_logprobs)?);
        } else if !content_logprobs.is_empty() {
            return Err(TransformError::shape(
                "Chat stream",
                "content logprobs without content delta",
            ));
        }
        if let Some(reasoning) = delta.reasoning_content {
            output.extend(self.reasoning_delta(reasoning, delta_rest.clone())?);
        }
        if let Some(refusal) = delta.refusal {
            output.extend(self.refusal_delta(choice.index, refusal, delta_rest.clone())?);
        }
        for call in delta.tool_calls.into_iter().flatten() {
            output.extend(self.tool_delta(call)?);
        }
        if let Some(function) = delta.function_call {
            output.extend(self.tool_delta(openai::ChatToolCallDelta {
                index: choice.index,
                id: Some(format!("call_{}", choice.index)),
                type_: Some(openai::ChatToolCallType::Function),
                function: Some(function),
                custom: None,
                rest: Default::default(),
            })?);
        }
        if !has_content && !has_reasoning && !has_refusal && !has_tools {
            merge_rest(&mut self.response_rest, delta_rest);
        }
        if choice.finish_reason.is_some() {
            self.finish_reason = choice.finish_reason;
        }
        Ok(output)
    }
}
