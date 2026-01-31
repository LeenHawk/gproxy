use bytes::Bytes;

use gproxy_provider_core::TrafficUsage;
use gproxy_protocol::claude::create_message::stream::BetaStreamEvent;
use gproxy_protocol::{gemini, openai};

use super::plan::UsageKind;
use super::stream::parse_gemini_stream_payload;

pub(super) fn extract_usage_for_kind(kind: UsageKind, body: &Bytes) -> Option<TrafficUsage> {
    match kind {
        UsageKind::ClaudeMessage => extract_claude_usage_from_body(body)
            .or_else(|| extract_gemini_usage_from_body(body).and_then(map_gemini_usage_to_claude)),
        UsageKind::OpenAIChat => extract_openai_chat_usage_from_body(body),
        UsageKind::OpenAIResponses => extract_openai_responses_usage_from_body(body).or_else(|| {
            extract_gemini_usage_from_body(body).and_then(map_gemini_usage_to_openai_responses)
        }),
        UsageKind::GeminiGenerate => extract_gemini_usage_from_body(body),
        UsageKind::None => None,
    }
}

fn extract_claude_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if let Some(usage) = value.get("usage") {
        let input_tokens = usage.get("input_tokens").and_then(|v| v.as_i64());
        let output_tokens = usage.get("output_tokens").and_then(|v| v.as_i64());
        let cache_creation_input_tokens = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64());
        let cache_read_input_tokens = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64());
        if input_tokens.is_some() || output_tokens.is_some() {
            let total_tokens = match (input_tokens, output_tokens) {
                (Some(input), Some(output)) => Some(input + output),
                _ => None,
            };
            return Some(TrafficUsage {
                claude_input_tokens: input_tokens,
                claude_output_tokens: output_tokens,
                claude_total_tokens: total_tokens,
                claude_cache_creation_input_tokens: cache_creation_input_tokens,
                claude_cache_read_input_tokens: cache_read_input_tokens,
                ..Default::default()
            });
        }
    }
    if let Some(tokens) = value.get("input_tokens").and_then(|v| v.as_i64()) {
        return Some(TrafficUsage {
            claude_input_tokens: Some(tokens),
            claude_total_tokens: Some(tokens),
            ..Default::default()
        });
    }
    None
}

fn extract_openai_chat_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = value.get("usage")?;
    let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_i64());
    let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_i64());
    let total_tokens = usage.get("total_tokens").and_then(|v| v.as_i64());
    if prompt_tokens.is_some() || completion_tokens.is_some() || total_tokens.is_some() {
        Some(TrafficUsage {
            openai_chat_prompt_tokens: prompt_tokens,
            openai_chat_completion_tokens: completion_tokens,
            openai_chat_total_tokens: total_tokens,
            ..Default::default()
        })
    } else {
        None
    }
}

fn extract_gemini_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    if let Ok(parsed) =
        serde_json::from_slice::<gemini::generate_content::response::GenerateContentResponse>(body)
        && let Some(usage) = parsed.usage_metadata.as_ref() {
            return Some(TrafficUsage {
                gemini_prompt_tokens: usage.prompt_token_count.map(|v| v as i64),
                gemini_candidates_tokens: usage.candidates_token_count.map(|v| v as i64),
                gemini_total_tokens: usage.total_token_count.map(|v| v as i64),
                gemini_cached_tokens: usage.cached_content_token_count.map(|v| v as i64),
                ..Default::default()
            });
        }
    extract_claude_usage_from_body(body).and_then(map_claude_usage_to_gemini)
}

fn extract_openai_responses_usage_from_body(body: &Bytes) -> Option<TrafficUsage> {
    if let Ok(parsed) = serde_json::from_slice::<openai::create_response::response::Response>(body)
        && let Some(usage) = parsed.usage.as_ref() {
            return Some(map_openai_responses_usage(usage));
        }
    extract_claude_usage_from_body(body).and_then(map_claude_usage_to_openai_responses)
}

fn map_openai_responses_usage(usage: &openai::create_response::types::ResponseUsage) -> TrafficUsage {
    TrafficUsage {
        openai_responses_input_tokens: Some(usage.input_tokens),
        openai_responses_output_tokens: Some(usage.output_tokens),
        openai_responses_total_tokens: Some(usage.total_tokens),
        openai_responses_input_cached_tokens: Some(usage.input_tokens_details.cached_tokens),
        openai_responses_output_reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
        ..Default::default()
    }
}

fn map_claude_usage_to_gemini(usage: TrafficUsage) -> Option<TrafficUsage> {
    let input_tokens = usage.claude_input_tokens;
    let output_tokens = usage.claude_output_tokens;
    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }
    let total_tokens = match (input_tokens, output_tokens) {
        (Some(input), Some(output)) => Some(input + output),
        _ => None,
    };
    Some(TrafficUsage {
        gemini_prompt_tokens: input_tokens,
        gemini_candidates_tokens: output_tokens,
        gemini_total_tokens: total_tokens,
        gemini_cached_tokens: usage.claude_cache_read_input_tokens,
        ..Default::default()
    })
}

fn map_claude_usage_to_openai_responses(usage: TrafficUsage) -> Option<TrafficUsage> {
    let input_tokens = usage.claude_input_tokens;
    let output_tokens = usage.claude_output_tokens;
    if input_tokens.is_none() && output_tokens.is_none() {
        return None;
    }
    let total_tokens = match (input_tokens, output_tokens) {
        (Some(input), Some(output)) => Some(input + output),
        _ => None,
    };
    Some(TrafficUsage {
        openai_responses_input_tokens: input_tokens,
        openai_responses_output_tokens: output_tokens,
        openai_responses_total_tokens: total_tokens,
        openai_responses_input_cached_tokens: usage.claude_cache_read_input_tokens,
        openai_responses_output_reasoning_tokens: None,
        ..Default::default()
    })
}

fn map_gemini_usage_to_claude(usage: TrafficUsage) -> Option<TrafficUsage> {
    let input_tokens = usage.gemini_prompt_tokens;
    let output_tokens = usage.gemini_candidates_tokens;
    if input_tokens.is_none() && output_tokens.is_none() && usage.gemini_total_tokens.is_none() {
        return None;
    }
    let total_tokens = usage
        .gemini_total_tokens
        .or_else(|| match (input_tokens, output_tokens) {
            (Some(input), Some(output)) => Some(input + output),
            _ => None,
        });
    Some(TrafficUsage {
        claude_input_tokens: input_tokens,
        claude_output_tokens: output_tokens,
        claude_total_tokens: total_tokens,
        claude_cache_read_input_tokens: usage.gemini_cached_tokens,
        ..Default::default()
    })
}

fn map_gemini_usage_to_openai_responses(usage: TrafficUsage) -> Option<TrafficUsage> {
    let input_tokens = usage.gemini_prompt_tokens;
    let output_tokens = usage.gemini_candidates_tokens;
    if input_tokens.is_none() && output_tokens.is_none() && usage.gemini_total_tokens.is_none() {
        return None;
    }
    let total_tokens = usage
        .gemini_total_tokens
        .or_else(|| match (input_tokens, output_tokens) {
            (Some(input), Some(output)) => Some(input + output),
            _ => None,
        });
    Some(TrafficUsage {
        openai_responses_input_tokens: input_tokens,
        openai_responses_output_tokens: output_tokens,
        openai_responses_total_tokens: total_tokens,
        openai_responses_input_cached_tokens: usage.gemini_cached_tokens,
        openai_responses_output_reasoning_tokens: None,
        ..Default::default()
    })
}

pub(super) fn map_usage_for_kind(kind: UsageKind, usage: Option<TrafficUsage>) -> Option<TrafficUsage> {
    let usage = usage?;
    match kind {
        UsageKind::ClaudeMessage => {
            if usage.claude_total_tokens.is_some()
                || usage.claude_input_tokens.is_some()
                || usage.claude_output_tokens.is_some()
            {
                Some(usage)
            } else {
                map_gemini_usage_to_claude(usage)
            }
        }
        UsageKind::GeminiGenerate => {
            if usage.gemini_total_tokens.is_some()
                || usage.gemini_prompt_tokens.is_some()
                || usage.gemini_candidates_tokens.is_some()
            {
                Some(usage)
            } else {
                map_claude_usage_to_gemini(usage)
            }
        }
        UsageKind::OpenAIResponses => {
            if usage.openai_responses_total_tokens.is_some()
                || usage.openai_responses_input_tokens.is_some()
                || usage.openai_responses_output_tokens.is_some()
            {
                Some(usage)
            } else if let Some(mapped) = map_gemini_usage_to_openai_responses(usage.clone()) {
                Some(mapped)
            } else {
                map_claude_usage_to_openai_responses(usage)
            }
        }
        _ => Some(usage),
    }
}

pub(super) struct ClaudeUsageState {
    state: gproxy_transform::stream2nostream::claude::ClaudeStreamToMessageState,
    usage: Option<TrafficUsage>,
}

impl ClaudeUsageState {
    pub(super) fn new() -> Self {
        Self {
            state: gproxy_transform::stream2nostream::claude::ClaudeStreamToMessageState::new(),
            usage: None,
        }
    }

    fn push_event(&mut self, data: &str) {
        if let Some(usage) = parse_claude_stream_usage(data) {
            self.usage = Some(usage);
        }
        if let Ok(parsed) = serde_json::from_str::<BetaStreamEvent>(data) {
            let _ = self.state.push_event(parsed);
        }
    }

    fn finish(mut self) -> Option<TrafficUsage> {
        if let Some(usage) = self.usage {
            return Some(usage);
        }
        let message = self.state.finalize_on_eof()?;
        let input_tokens = message.usage.input_tokens as i64;
        let output_tokens = message.usage.output_tokens as i64;
        Some(TrafficUsage {
            claude_input_tokens: Some(input_tokens),
            claude_output_tokens: Some(output_tokens),
            claude_total_tokens: Some(input_tokens + output_tokens),
            claude_cache_creation_input_tokens: Some(message.usage.cache_creation_input_tokens as i64),
            claude_cache_read_input_tokens: Some(message.usage.cache_read_input_tokens as i64),
            ..Default::default()
        })
    }
}

fn parse_claude_stream_usage(data: &str) -> Option<TrafficUsage> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    let usage = value
        .get("usage")
        .or_else(|| value.get("message").and_then(|message| message.get("usage")))?;
    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_i64());
    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_i64());
    let cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64());
    let cache_read_input_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64());
    if input_tokens.is_some() || output_tokens.is_some() {
        let total_tokens = match (input_tokens, output_tokens) {
            (Some(input), Some(output)) => Some(input + output),
            _ => None,
        };
        return Some(TrafficUsage {
            claude_input_tokens: input_tokens,
            claude_output_tokens: output_tokens,
            claude_total_tokens: total_tokens,
            claude_cache_creation_input_tokens: cache_creation_input_tokens,
            claude_cache_read_input_tokens: cache_read_input_tokens,
            ..Default::default()
        });
    }
    None
}

pub(super) struct OpenAIUsageState {
    usage: Option<TrafficUsage>,
}

impl OpenAIUsageState {
    pub(super) fn new() -> Self {
        Self { usage: None }
    }

    fn push_event(&mut self, data: &str) {
        if self.usage.is_some() || data == "[DONE]" {
            return;
        }
        if let Ok(parsed) = serde_json::from_str::<
            openai::create_chat_completions::stream::CreateChatCompletionStreamResponse,
        >(data)
            && let Some(stream_usage) = parsed.usage {
                self.usage = Some(TrafficUsage {
                    openai_chat_prompt_tokens: Some(stream_usage.prompt_tokens),
                    openai_chat_completion_tokens: Some(stream_usage.completion_tokens),
                    openai_chat_total_tokens: Some(stream_usage.total_tokens),
                    ..Default::default()
                });
                return;
            }

        let value: serde_json::Value = match serde_json::from_str(data) {
            Ok(value) => value,
            Err(_) => return,
        };
        let usage = match value.get("usage") {
            Some(usage) => usage,
            None => return,
        };
        let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_i64());
        let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_i64());
        let total_tokens = usage.get("total_tokens").and_then(|v| v.as_i64());
        if prompt_tokens.is_some() || completion_tokens.is_some() || total_tokens.is_some() {
            self.usage = Some(TrafficUsage {
                openai_chat_prompt_tokens: prompt_tokens,
                openai_chat_completion_tokens: completion_tokens,
                openai_chat_total_tokens: total_tokens,
                ..Default::default()
            });
        }
    }

    fn finish(self) -> Option<TrafficUsage> {
        self.usage
    }
}

pub(super) struct OpenAIResponsesUsageState {
    usage: Option<TrafficUsage>,
}

impl OpenAIResponsesUsageState {
    pub(super) fn new() -> Self {
        Self { usage: None }
    }

    fn push_event(&mut self, data: &str) {
        if self.usage.is_some() || data == "[DONE]" {
            return;
        }
        if let Ok(parsed) = serde_json::from_str::<openai::create_response::stream::ResponseStreamEvent>(data) {
            let response = match parsed {
                openai::create_response::stream::ResponseStreamEvent::Completed(event) => {
                    Some(event.response)
                }
                openai::create_response::stream::ResponseStreamEvent::Created(event) => {
                    Some(event.response)
                }
                openai::create_response::stream::ResponseStreamEvent::InProgress(event) => {
                    Some(event.response)
                }
                openai::create_response::stream::ResponseStreamEvent::Failed(event) => {
                    Some(event.response)
                }
                openai::create_response::stream::ResponseStreamEvent::Incomplete(event) => {
                    Some(event.response)
                }
                _ => None,
            };
            if let Some(response) = response
                && let Some(usage) = response.usage.as_ref() {
                    self.usage = Some(map_openai_responses_usage(usage));
                }
        }
    }

    fn finish(self) -> Option<TrafficUsage> {
        self.usage
    }
}

pub(super) struct GeminiUsageState {
    usage: Option<TrafficUsage>,
}

impl GeminiUsageState {
    pub(super) fn new() -> Self {
        Self { usage: None }
    }

    fn push_event(&mut self, data: &str) {
        if data == "[DONE]" {
            return;
        }
        for parsed in parse_gemini_stream_payload(data) {
            if let Some(usage) = parsed.usage_metadata.as_ref() {
                self.usage = Some(TrafficUsage {
                    gemini_prompt_tokens: usage.prompt_token_count.map(|v| v as i64),
                    gemini_candidates_tokens: usage.candidates_token_count.map(|v| v as i64),
                    gemini_total_tokens: usage.total_token_count.map(|v| v as i64),
                    gemini_cached_tokens: usage.cached_content_token_count.map(|v| v as i64),
                    ..Default::default()
                });
            }
        }
    }

    fn finish(self) -> Option<TrafficUsage> {
        self.usage
    }
}

#[allow(clippy::large_enum_variant)]
pub(super) enum UsageState {
    Claude(ClaudeUsageState),
    OpenAI(OpenAIUsageState),
    OpenAIResponses(OpenAIResponsesUsageState),
    Gemini(GeminiUsageState),
}

impl UsageState {
    pub(super) fn push_event(&mut self, data: &str) {
        match self {
            UsageState::Claude(state) => state.push_event(data),
            UsageState::OpenAI(state) => state.push_event(data),
            UsageState::OpenAIResponses(state) => state.push_event(data),
            UsageState::Gemini(state) => state.push_event(data),
        }
    }

    pub(super) fn finish(self) -> Option<TrafficUsage> {
        match self {
            UsageState::Claude(state) => state.finish(),
            UsageState::OpenAI(state) => state.finish(),
            UsageState::OpenAIResponses(state) => state.finish(),
            UsageState::Gemini(state) => state.finish(),
        }
    }
}
