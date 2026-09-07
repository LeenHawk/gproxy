use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};
use crate::models::common::wire_string;

use crate::generate_content::openai_chat_to_gemini_generate_content::{content, wire};

mod tools;

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

#[derive(Default)]
pub(crate) struct State {
    tools: BTreeMap<(u32, u32), tools::Pending>,
    seen: BTreeSet<u32>,
    finished: BTreeSet<u32>,
    stopped: bool,
}

impl State {
    pub(crate) fn push_typed(
        &mut self,
        input: openai::ChatCompletionChunk,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        if self.stopped {
            return Err(TransformError::shape("Chat stream", "chunk after finish"));
        }
        let model = wire_string(&input.model)?;
        let response_id = input.id;
        let mut usage = input.usage.map(wire::usage).transpose()?;
        if let Some(usage) = usage.as_mut() {
            usage.service_tier = wire::service_tier(input.service_tier.clone());
        }
        let mut candidates = Vec::new();
        for choice in input.choices {
            if self.finished.contains(&choice.index) {
                return Err(TransformError::shape(
                    "Chat stream",
                    "choice frame after finish_reason",
                ));
            }
            self.seen.insert(choice.index);
            let mut parts = delta_parts(
                choice.delta.content,
                choice.delta.reasoning_content,
                choice.delta.refusal,
            );
            if let Some(call) = choice.delta.function_call {
                tools::update_legacy(&mut self.tools, choice.index, call)?;
            }
            for call in choice.delta.tool_calls.into_iter().flatten() {
                tools::update(&mut self.tools, choice.index, call)?;
            }
            let finish_reason = choice.finish_reason.map(wire::finish_reason).transpose()?;
            if finish_reason.is_some() {
                parts.extend(tools::finish_choice(&mut self.tools, choice.index)?);
                self.finished.insert(choice.index);
            }
            candidates.push(crate::wire!(gemini::Candidate {
                content: (!parts.is_empty()).then_some(gemini::Content {
                    parts,
                    role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
                    rest: Default::default(),
                }),
                finish_reason,
                safety_ratings: Vec::new(),
                citation_metadata: None,
                token_count: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(wire::index(choice.index)?),
                finish_message: None,
                rest: Default::default(),
            }));
        }
        let output = crate::wire!(gemini::GenerateContentResponse {
            candidates,
            prompt_feedback: None,
            usage_metadata: usage,
            model_version: Some(model),
            response_id: Some(response_id),
            model_status: None,
            rest: Default::default(),
        });
        Ok(vec![output])
    }

    pub(crate) fn finish_typed(
        &mut self,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        if self.seen.is_empty() || self.seen != self.finished || !self.tools.is_empty() {
            return Err(TransformError::IncompleteStream);
        }
        self.stopped = true;
        Ok(Vec::new())
    }
}

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if frame.data == "[DONE]" {
            return self.finish_typed().map(|_| Vec::new());
        }
        if self.stopped {
            return Err(TransformError::shape("Chat stream", "frame after [DONE]"));
        }
        self.push_typed(serde_json::from_str(&frame.data)?)?
            .into_iter()
            .map(|event| SseFrame::typed(None, &event))
            .collect()
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}

fn delta_parts(
    text: Option<String>,
    reasoning: Option<String>,
    refusal: Option<String>,
) -> Vec<gemini::Part> {
    let mut parts = Vec::new();
    if let Some(reasoning) = reasoning.filter(|value| !value.is_empty()) {
        parts.push(content::text_part(reasoning, true));
    }
    if let Some(text) = text.filter(|value| !value.is_empty()) {
        parts.push(content::text_part(text, false));
    }
    if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
        parts.push(content::text_part(refusal, false));
    }
    parts
}
