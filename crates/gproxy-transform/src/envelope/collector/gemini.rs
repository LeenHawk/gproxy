use std::collections::BTreeMap;

use gproxy_protocol::gemini;

use super::SseFrame;
use crate::TransformError;

#[derive(Default)]
pub(super) struct GeminiCollector {
    candidates: BTreeMap<i32, gemini::Candidate>,
    prompt_feedback: Option<gemini::PromptFeedback>,
    usage: Option<gemini::UsageMetadata>,
    model_version: Option<String>,
    response_id: Option<String>,
    model_status: Option<gemini::ModelStatus>,
}

impl GeminiCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        let chunk: gemini::GenerateContentResponse = serde_json::from_str(&frame.data)?;
        for (fallback, candidate) in chunk.candidates.into_iter().enumerate() {
            let index = match candidate.index {
                Some(index) if index >= 0 => index,
                Some(_) => {
                    return Err(TransformError::shape(
                        "Gemini stream",
                        "candidate index is negative",
                    ));
                }
                None => i32::try_from(fallback).map_err(|_| {
                    TransformError::shape("Gemini stream", "candidate index exceeds i32")
                })?,
            };
            let target = self.candidates.entry(index).or_default();
            if target.finish_reason.is_some() {
                return Err(TransformError::shape(
                    "Gemini stream",
                    "candidate data followed finishReason",
                ));
            }
            merge_candidate(target, candidate);
        }
        if chunk.prompt_feedback.is_some() {
            self.prompt_feedback = chunk.prompt_feedback;
        }
        if let Some(usage) = chunk.usage_metadata {
            merge_usage(self.usage.get_or_insert_with(Default::default), usage);
        }
        set_identity(&mut self.model_version, chunk.model_version, "modelVersion")?;
        set_identity(&mut self.response_id, chunk.response_id, "responseId")?;
        self.model_status = chunk.model_status.or(self.model_status.take());
        Ok(())
    }

    pub(super) fn is_complete(&self) -> bool {
        (!self.candidates.is_empty()
            && self
                .candidates
                .values()
                .all(|candidate| candidate.finish_reason.is_some()))
            || (self.candidates.is_empty()
                && self
                    .prompt_feedback
                    .as_ref()
                    .and_then(|feedback| feedback.block_reason.as_ref())
                    .is_some())
    }

    pub(super) fn finish(self) -> Result<gemini::GenerateContentResponse, TransformError> {
        if !self.is_complete() {
            return Err(TransformError::IncompleteStream);
        }
        Ok(crate::wire!(gemini::GenerateContentResponse {
            candidates: self.candidates.into_values().collect(),
            prompt_feedback: self.prompt_feedback,
            usage_metadata: self.usage,
            model_version: self.model_version,
            response_id: self.response_id,
            model_status: self.model_status,
            rest: Default::default(),
        }))
    }
}

fn set_identity(
    target: &mut Option<String>,
    update: Option<String>,
    field: &'static str,
) -> Result<(), TransformError> {
    if let Some(update) = update {
        if target.as_ref().is_some_and(|current| current != &update) {
            return Err(TransformError::shape(
                "Gemini stream",
                format!("{field} changed during the stream"),
            ));
        }
        *target = Some(update);
    }
    Ok(())
}

fn merge_candidate(target: &mut gemini::Candidate, update: gemini::Candidate) {
    match (&mut target.content, update.content) {
        (Some(target), Some(update)) => {
            target.parts.extend(update.parts);
            target.role = update.role.or(target.role.take());
        }
        (slot @ None, Some(update)) => *slot = Some(update),
        _ => {}
    }
    target.finish_reason = update.finish_reason.or(target.finish_reason.take());
    target.safety_ratings.extend(update.safety_ratings);
    target.citation_metadata = update.citation_metadata.or(target.citation_metadata.take());
    target.token_count = update.token_count.or(target.token_count);
    target.grounding_metadata = update
        .grounding_metadata
        .or(target.grounding_metadata.take());
    target.avg_logprobs = update.avg_logprobs.or(target.avg_logprobs);
    target.logprobs_result = update.logprobs_result.or(target.logprobs_result.take());
    target.url_context_metadata = update
        .url_context_metadata
        .or(target.url_context_metadata.take());
    target.index = update.index.or(target.index);
    target.finish_message = update.finish_message.or(target.finish_message.take());
}

fn merge_usage(target: &mut gemini::UsageMetadata, update: gemini::UsageMetadata) {
    target.prompt_token_count = update.prompt_token_count.or(target.prompt_token_count);
    target.cached_content_token_count = update
        .cached_content_token_count
        .or(target.cached_content_token_count);
    target.candidates_token_count = update
        .candidates_token_count
        .or(target.candidates_token_count);
    target.tool_use_prompt_token_count = update
        .tool_use_prompt_token_count
        .or(target.tool_use_prompt_token_count);
    target.thoughts_token_count = update.thoughts_token_count.or(target.thoughts_token_count);
    target.total_token_count = update.total_token_count.or(target.total_token_count);
    replace_if_present(
        &mut target.prompt_tokens_details,
        update.prompt_tokens_details,
    );
    replace_if_present(
        &mut target.cache_tokens_details,
        update.cache_tokens_details,
    );
    replace_if_present(
        &mut target.candidates_tokens_details,
        update.candidates_tokens_details,
    );
    replace_if_present(
        &mut target.tool_use_prompt_tokens_details,
        update.tool_use_prompt_tokens_details,
    );
    target.service_tier = update.service_tier.or(target.service_tier.take());
}

fn replace_if_present<T>(target: &mut Vec<T>, update: Vec<T>) {
    if !update.is_empty() {
        *target = update;
    }
}
