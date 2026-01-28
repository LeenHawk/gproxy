use std::collections::BTreeMap;

use gproxy_protocol::gemini::count_tokens::types::{
    Content, FunctionCall, FunctionResponse, Part,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{
    Candidate, FinishReason, ModelStatus, PromptFeedback, UsageMetadata,
};
use gproxy_protocol::gemini::stream_content::response::StreamGenerateContentResponse;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
pub struct GeminiStreamToResponseState {
    candidates: BTreeMap<u32, Candidate>,
    prompt_feedback: Option<PromptFeedback>,
    usage_metadata: Option<UsageMetadata>,
    model_version: Option<String>,
    response_id: Option<String>,
    model_status: Option<ModelStatus>,
}

impl GeminiStreamToResponseState {
    pub fn new() -> Self {
        Self {
            candidates: BTreeMap::new(),
            prompt_feedback: None,
            usage_metadata: None,
            model_version: None,
            response_id: None,
            model_status: None,
        }
    }

    pub fn push_chunk(
        &mut self,
        chunk: StreamGenerateContentResponse,
    ) -> Option<GenerateContentResponse> {
        self.merge_metadata(&chunk);
        for (idx, candidate) in chunk.candidates.into_iter().enumerate() {
            let index = candidate.index.unwrap_or(idx as u32);
            self.merge_candidate(index, candidate);
        }

        if self.is_finished() {
            Some(self.build_response())
        } else {
            None
        }
    }

    pub fn finalize(&self) -> GenerateContentResponse {
        self.build_response_with_finish_fallback(FinishReason::Stop)
    }

    pub fn finalize_on_eof(&self) -> GenerateContentResponse {
        self.build_response_with_finish_fallback(FinishReason::Other)
    }

    fn merge_metadata(&mut self, chunk: &GenerateContentResponse) {
        if chunk.prompt_feedback.is_some() {
            self.prompt_feedback = chunk.prompt_feedback.clone();
        }
        if chunk.usage_metadata.is_some() {
            self.usage_metadata = chunk.usage_metadata.clone();
        }
        if chunk.model_version.is_some() {
            self.model_version = chunk.model_version.clone();
        }
        if chunk.response_id.is_some() {
            self.response_id = chunk.response_id.clone();
        }
        if chunk.model_status.is_some() {
            self.model_status = chunk.model_status.clone();
        }
    }

    fn merge_candidate(&mut self, index: u32, incoming: Candidate) {
        let entry = self.candidates.entry(index).or_insert_with(|| {
            let mut candidate = incoming.clone();
            candidate.index = Some(index);
            candidate
        });

        merge_content(&mut entry.content, incoming.content);
        if incoming.finish_reason.is_some() {
            entry.finish_reason = incoming.finish_reason;
        }
        if incoming.safety_ratings.is_some() {
            entry.safety_ratings = incoming.safety_ratings;
        }
        if incoming.citation_metadata.is_some() {
            entry.citation_metadata = incoming.citation_metadata;
        }
        if incoming.token_count.is_some() {
            entry.token_count = incoming.token_count;
        }
        if incoming.grounding_attributions.is_some() {
            entry.grounding_attributions = incoming.grounding_attributions;
        }
        if incoming.grounding_metadata.is_some() {
            entry.grounding_metadata = incoming.grounding_metadata;
        }
        if incoming.avg_logprobs.is_some() {
            entry.avg_logprobs = incoming.avg_logprobs;
        }
        if incoming.logprobs_result.is_some() {
            entry.logprobs_result = incoming.logprobs_result;
        }
        if incoming.url_context_metadata.is_some() {
            entry.url_context_metadata = incoming.url_context_metadata;
        }
        if incoming.finish_message.is_some() {
            entry.finish_message = incoming.finish_message;
        }
    }

    fn build_response(&self) -> GenerateContentResponse {
        let candidates = self
            .candidates
            .iter()
            .map(|(index, candidate)| {
                let mut candidate = candidate.clone();
                if candidate.index.is_none() {
                    candidate.index = Some(*index);
                }
                candidate
            })
            .collect();

        GenerateContentResponse {
            candidates,
            prompt_feedback: self.prompt_feedback.clone(),
            usage_metadata: self.usage_metadata.clone(),
            model_version: self.model_version.clone(),
            response_id: self.response_id.clone(),
            model_status: self.model_status.clone(),
        }
    }

    fn build_response_with_finish_fallback(
        &self,
        fallback: FinishReason,
    ) -> GenerateContentResponse {
        let candidates = self
            .candidates
            .iter()
            .map(|(index, candidate)| {
                let mut candidate = candidate.clone();
                if candidate.index.is_none() {
                    candidate.index = Some(*index);
                }
                if candidate.finish_reason.is_none() {
                    candidate.finish_reason = Some(fallback);
                }
                candidate
            })
            .collect();

        GenerateContentResponse {
            candidates,
            prompt_feedback: self.prompt_feedback.clone(),
            usage_metadata: self.usage_metadata.clone(),
            model_version: self.model_version.clone(),
            response_id: self.response_id.clone(),
            model_status: self.model_status.clone(),
        }
    }

    fn is_finished(&self) -> bool {
        if self.candidates.is_empty() {
            return false;
        }
        self.candidates
            .values()
            .all(|candidate| candidate.finish_reason.is_some())
    }
}

impl Default for GeminiStreamToResponseState {
    fn default() -> Self {
        Self::new()
    }
}

fn merge_content(existing: &mut Content, incoming: Content) {
    if incoming.role.is_some() {
        existing.role = incoming.role;
    }
    for part in incoming.parts {
        merge_part(&mut existing.parts, part);
    }
}

fn merge_part(parts: &mut Vec<Part>, mut incoming: Part) {
    if let Some(text) = incoming.text.take() {
        if let Some(last) = parts.last_mut()
            && last.text.is_some()
            && last.inline_data.is_none()
            && last.function_call.is_none()
            && last.function_response.is_none()
            && last.file_data.is_none()
            && last.executable_code.is_none()
            && last.code_execution_result.is_none()
        {
            if let Some(last_text) = last.text.as_mut() {
                last_text.push_str(&text);
            }
            merge_part_metadata(last, &incoming);
            return;
        }

        incoming.text = Some(text);
        parts.push(incoming);
        return;
    }

    if let Some(executable_code) = incoming.executable_code.take() {
        if let Some(last) = parts.last_mut()
            && let Some(last_code) = last.executable_code.as_mut()
                && last_code.language == executable_code.language
            {
                last_code.code.push_str(&executable_code.code);
                merge_part_metadata(last, &incoming);
                return;
            }
        incoming.executable_code = Some(executable_code);
        parts.push(incoming);
        return;
    }

    if let Some(function_call) = incoming.function_call.take() {
        if let Some(last) = parts.last_mut()
            && let Some(last_call) = last.function_call.as_mut()
                && last_call.name == function_call.name
            {
                merge_function_call(last_call, function_call);
                merge_part_metadata(last, &incoming);
                return;
            }
        incoming.function_call = Some(function_call);
        parts.push(incoming);
        return;
    }

    if let Some(function_response) = incoming.function_response.take() {
        if let Some(last) = parts.last_mut()
            && let Some(last_response) = last.function_response.as_mut()
                && last_response.name == function_response.name
            {
                merge_function_response(last_response, function_response);
                merge_part_metadata(last, &incoming);
                return;
            }
        incoming.function_response = Some(function_response);
        parts.push(incoming);
        return;
    }

    parts.push(incoming);
}

fn merge_part_metadata(target: &mut Part, incoming: &Part) {
    if target.thought.is_none() {
        target.thought = incoming.thought;
    }
    if target.thought_signature.is_none() {
        target.thought_signature = incoming.thought_signature.clone();
    }
    if incoming.part_metadata.is_some() {
        target.part_metadata = incoming.part_metadata.clone();
    }
    if incoming.video_metadata.is_some() {
        target.video_metadata = incoming.video_metadata.clone();
    }
}

fn merge_function_call(target: &mut FunctionCall, incoming: FunctionCall) {
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    if target.name.is_empty() {
        target.name = incoming.name;
    }
    if let Some(args) = incoming.args {
        target.args = Some(merge_json(target.args.take(), args));
    }
}

fn merge_function_response(target: &mut FunctionResponse, incoming: FunctionResponse) {
    if incoming.id.is_some() {
        target.id = incoming.id;
    }
    if target.name.is_empty() {
        target.name = incoming.name;
    }
    target.response = incoming.response;
    if incoming.parts.is_some() {
        target.parts = incoming.parts;
    }
    if incoming.will_continue.is_some() {
        target.will_continue = incoming.will_continue;
    }
    if incoming.scheduling.is_some() {
        target.scheduling = incoming.scheduling;
    }
}

fn merge_json(existing: Option<JsonValue>, incoming: JsonValue) -> JsonValue {
    match (existing, incoming) {
        (Some(JsonValue::Object(mut base)), JsonValue::Object(update)) => {
            for (key, value) in update {
                base.insert(key, value);
            }
            JsonValue::Object(base)
        }
        (_, value) => value,
    }
}
