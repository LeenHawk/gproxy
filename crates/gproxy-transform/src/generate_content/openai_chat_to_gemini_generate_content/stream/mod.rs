use std::collections::BTreeSet;

use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

use crate::generate_content::gemini_generate_content_to_openai_chat::wire;

mod parts;
mod tools;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
struct State {
    id: Option<String>,
    model: Option<openai::OpenAiModelId>,
    started: BTreeSet<u32>,
    seen: BTreeSet<u32>,
    finished: BTreeSet<u32>,
    tool_candidates: BTreeSet<u32>,
    tools: tools::State,
}

impl State {
    fn chunk(
        &mut self,
        input: gemini::GenerateContentResponse,
    ) -> Result<Vec<Bytes>, TransformError> {
        if let Some(id) = input.response_id {
            self.id = Some(id);
        }
        if let Some(model) = input.model_version {
            self.model = Some(model.into());
        }
        let service_tier = match input
            .usage_metadata
            .as_ref()
            .and_then(|usage| usage.service_tier.clone())
        {
            Some(tier) => wire::service_tier(Some(tier))?,
            None => None,
        };
        let usage = input.usage_metadata.map(wire::usage).transpose()?;
        let blocked = input
            .prompt_feedback
            .as_ref()
            .and_then(|feedback| feedback.block_reason.as_ref())
            .is_some();
        let mut choices = Vec::new();
        for (fallback, candidate) in input.candidates.into_iter().enumerate() {
            let index = match candidate.index {
                Some(index) => wire::count(index, "candidate.index")?,
                None => u32::try_from(fallback).map_err(|_| {
                    TransformError::shape("Gemini stream", "candidate index exceeds u32")
                })?,
            };
            if self.finished.contains(&index) {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "candidate frame after finishReason",
                ));
            }
            self.seen.insert(index);
            let (mut delta, has_tool) = parts::convert(candidate.content, index, &mut self.tools)?;
            if has_tool {
                self.tool_candidates.insert(index);
            }
            if self.started.insert(index) {
                delta.role = Some(openai::ChatDeltaRole::Assistant);
            }
            let mut finish_reason = candidate
                .finish_reason
                .map(wire::finish_reason)
                .transpose()?;
            if finish_reason == Some(openai::ChatFinishReason::Stop)
                && self.tool_candidates.contains(&index)
            {
                finish_reason = Some(openai::ChatFinishReason::ToolCalls);
            }
            if finish_reason.is_some() {
                self.finished.insert(index);
            }
            choices.push(openai::ChatChunkChoice {
                index,
                delta,
                finish_reason,
                logprobs: None,
                rest: Default::default(),
            });
        }
        if choices.is_empty() && blocked {
            self.seen.insert(0);
            self.finished.insert(0);
            choices.push(openai::ChatChunkChoice {
                index: 0,
                delta: empty_delta(Some(openai::ChatDeltaRole::Assistant)),
                finish_reason: Some(openai::ChatFinishReason::ContentFilter),
                logprobs: None,
                rest: Default::default(),
            });
        }
        let output =
            openai::ChatCompletionChunk {
                id: self.id.clone().ok_or_else(|| {
                    TransformError::shape("Gemini stream", "responseId is missing")
                })?,
                choices,
                created: None,
                model: self.model.clone().ok_or_else(|| {
                    TransformError::shape("Gemini stream", "modelVersion is missing")
                })?,
                object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
                service_tier,
                system_fingerprint: None,
                usage,
                rest: Default::default(),
            };
        Ok(vec![SseFrame::typed(None, &output)?])
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.chunk(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.seen.is_empty() || self.seen != self.finished || !self.tools.complete() {
            return Err(TransformError::IncompleteStream);
        }
        Ok(vec![SseFrame::encode(None, "[DONE]")])
    }
}

fn empty_delta(role: Option<openai::ChatDeltaRole>) -> openai::ChatDelta {
    openai::ChatDelta {
        role,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        rest: Default::default(),
    }
}
