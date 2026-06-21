use std::collections::BTreeMap;

use crate::protocol::gemini;

pub fn response(
    chunks: impl IntoIterator<Item = gemini::StreamGenerateContentChunk>,
) -> gemini::GenerateContentResponse {
    let mut collector = GeminiCollector::default();
    for chunk in chunks {
        collector.push(chunk);
    }
    collector.finish()
}

#[derive(Default)]
struct GeminiCollector {
    candidates: BTreeMap<i32, CandidateState>,
    prompt_feedback: Option<gemini::PromptFeedback>,
    usage_metadata: Option<gemini::UsageMetadata>,
    model_version: Option<String>,
    response_id: Option<String>,
    model_status: Option<gemini::ModelStatus>,
}

impl GeminiCollector {
    fn push(&mut self, chunk: gemini::GenerateContentResponse) {
        self.prompt_feedback = chunk.prompt_feedback.or(self.prompt_feedback.take());
        self.usage_metadata = chunk.usage_metadata.or(self.usage_metadata.take());
        self.model_version = chunk.model_version.or(self.model_version.take());
        self.response_id = chunk.response_id.or(self.response_id.take());
        self.model_status = chunk.model_status.or(self.model_status.take());

        for (fallback_index, candidate) in chunk.candidates.into_iter().enumerate() {
            let index = candidate
                .index
                .unwrap_or_else(|| usize_to_i32(fallback_index));
            self.candidates
                .entry(index)
                .or_insert_with(|| CandidateState::new(index))
                .push(candidate);
        }
    }

    fn finish(self) -> gemini::GenerateContentResponse {
        gemini::GenerateContentResponse {
            candidates: self
                .candidates
                .into_values()
                .map(CandidateState::finish)
                .collect(),
            prompt_feedback: self.prompt_feedback,
            usage_metadata: self.usage_metadata,
            model_version: self.model_version,
            response_id: self.response_id,
            model_status: self.model_status,
            extra: Default::default(),
        }
    }
}

struct CandidateState {
    index: i32,
    content: Option<gemini::Content>,
    finish_reason: Option<gemini::FinishReason>,
    safety_ratings: Vec<gemini::SafetyRating>,
    citation_metadata: Option<gemini::CitationMetadata>,
    token_count: Option<i32>,
    grounding_metadata: Option<gemini::GroundingMetadata>,
    avg_logprobs: Option<f64>,
    logprobs_result: Option<gemini::LogprobsResult>,
    url_context_metadata: Option<gemini::UrlContextMetadata>,
    finish_message: Option<String>,
}

impl CandidateState {
    fn new(index: i32) -> Self {
        Self {
            index,
            content: None,
            finish_reason: None,
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            finish_message: None,
        }
    }

    fn push(&mut self, candidate: gemini::Candidate) {
        if let Some(content) = candidate.content {
            append_content(&mut self.content, content);
        }
        self.finish_reason = candidate.finish_reason.or(self.finish_reason.take());
        self.safety_ratings.extend(candidate.safety_ratings);
        self.citation_metadata = candidate
            .citation_metadata
            .or(self.citation_metadata.take());
        self.token_count = candidate.token_count.or(self.token_count);
        self.grounding_metadata = candidate
            .grounding_metadata
            .or(self.grounding_metadata.take());
        self.avg_logprobs = candidate.avg_logprobs.or(self.avg_logprobs);
        self.logprobs_result = candidate.logprobs_result.or(self.logprobs_result.take());
        self.url_context_metadata = candidate
            .url_context_metadata
            .or(self.url_context_metadata.take());
        self.finish_message = candidate.finish_message.or(self.finish_message.take());
    }

    fn finish(self) -> gemini::Candidate {
        gemini::Candidate {
            content: self.content,
            finish_reason: self.finish_reason,
            safety_ratings: self.safety_ratings,
            citation_metadata: self.citation_metadata,
            token_count: self.token_count,
            grounding_metadata: self.grounding_metadata,
            avg_logprobs: self.avg_logprobs,
            logprobs_result: self.logprobs_result,
            url_context_metadata: self.url_context_metadata,
            index: Some(self.index),
            finish_message: self.finish_message,
            extra: Default::default(),
        }
    }
}

fn append_content(target: &mut Option<gemini::Content>, incoming: gemini::Content) {
    if let Some(target) = target {
        target.parts.extend(incoming.parts);
        target.role = incoming.role.or(target.role.take());
        target.extra = Default::default();
    } else {
        *target = Some(gemini::Content {
            parts: incoming.parts,
            role: incoming.role,
            extra: Default::default(),
        });
    }
}

fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}
