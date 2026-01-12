use time::OffsetDateTime;

use crate::formats::claude::messages::BetaUsage;
use crate::formats::claude::stream::ClaudeStreamUsage;
use crate::formats::gemini::types::UsageMetadata;
use crate::formats::openai::chat_completions::CompletionUsage;
use crate::formats::openai::chat_completions::{
    ChatCompletionStreamOptions, CreateChatCompletionRequest,
};
use crate::formats::openai::responses::ResponseUsage;
use crate::providers::credential_status::ProviderKind;
use crate::providers::usage::UsageRecord;

fn now_ts() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

pub(crate) fn build_openai_chat_usage_record(
    provider: ProviderKind,
    request_id: &str,
    created_at: i64,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: CompletionUsage,
) -> UsageRecord {
    UsageRecord {
        provider,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("openai".to_string()),
        request_id: Some(request_id.to_string()),
        created_at,
        oa_resp_input_tokens: None,
        oa_resp_output_tokens: None,
        oa_resp_total_tokens: None,
        oa_resp_input_tokens_details_cached_tokens: None,
        oa_resp_output_tokens_details_reasoning_tokens: None,
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: Some(usage.prompt_tokens),
        oa_chat_completion_tokens: Some(usage.completion_tokens),
        oa_chat_total_tokens: Some(usage.total_tokens),
        oa_chat_prompt_tokens_details_cached_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens),
        oa_chat_completion_tokens_details_reasoning_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens),
        oa_chat_completion_tokens_details_audio_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.audio_tokens),
        claude_input_tokens: None,
        claude_output_tokens: None,
        claude_cache_creation_input_tokens: None,
        claude_cache_read_input_tokens: None,
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}

pub(crate) fn build_claude_usage_record(
    provider: ProviderKind,
    request_id: Option<&str>,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: BetaUsage,
) -> UsageRecord {
    UsageRecord {
        provider,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("claude".to_string()),
        request_id: request_id.map(str::to_string),
        created_at: now_ts(),
        oa_resp_input_tokens: None,
        oa_resp_output_tokens: None,
        oa_resp_total_tokens: None,
        oa_resp_input_tokens_details_cached_tokens: None,
        oa_resp_output_tokens_details_reasoning_tokens: None,
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: None,
        oa_chat_completion_tokens: None,
        oa_chat_total_tokens: None,
        oa_chat_prompt_tokens_details_cached_tokens: None,
        oa_chat_completion_tokens_details_reasoning_tokens: None,
        oa_chat_completion_tokens_details_audio_tokens: None,
        claude_input_tokens: Some(usage.input_tokens as i64),
        claude_output_tokens: Some(usage.output_tokens as i64),
        claude_cache_creation_input_tokens: Some(usage.cache_creation_input_tokens as i64),
        claude_cache_read_input_tokens: Some(usage.cache_read_input_tokens as i64),
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}

pub(crate) fn build_claude_stream_usage_record(
    provider: ProviderKind,
    request_id: Option<&str>,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: ClaudeStreamUsage,
) -> UsageRecord {
    UsageRecord {
        provider,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("claude".to_string()),
        request_id: request_id.map(str::to_string),
        created_at: now_ts(),
        oa_resp_input_tokens: None,
        oa_resp_output_tokens: None,
        oa_resp_total_tokens: None,
        oa_resp_input_tokens_details_cached_tokens: None,
        oa_resp_output_tokens_details_reasoning_tokens: None,
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: None,
        oa_chat_completion_tokens: None,
        oa_chat_total_tokens: None,
        oa_chat_prompt_tokens_details_cached_tokens: None,
        oa_chat_completion_tokens_details_reasoning_tokens: None,
        oa_chat_completion_tokens_details_audio_tokens: None,
        claude_input_tokens: usage.input_tokens.map(|value| value as i64),
        claude_output_tokens: usage.output_tokens.map(|value| value as i64),
        claude_cache_creation_input_tokens: usage.cache_creation_input_tokens.map(|value| value as i64),
        claude_cache_read_input_tokens: usage.cache_read_input_tokens.map(|value| value as i64),
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}

pub(crate) fn build_gemini_usage_record(
    provider: ProviderKind,
    request_id: Option<&str>,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: UsageMetadata,
) -> UsageRecord {
    UsageRecord {
        provider,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("gemini".to_string()),
        request_id: request_id.map(str::to_string),
        created_at: now_ts(),
        oa_resp_input_tokens: None,
        oa_resp_output_tokens: None,
        oa_resp_total_tokens: None,
        oa_resp_input_tokens_details_cached_tokens: None,
        oa_resp_output_tokens_details_reasoning_tokens: None,
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: None,
        oa_chat_completion_tokens: None,
        oa_chat_total_tokens: None,
        oa_chat_prompt_tokens_details_cached_tokens: None,
        oa_chat_completion_tokens_details_reasoning_tokens: None,
        oa_chat_completion_tokens_details_audio_tokens: None,
        claude_input_tokens: None,
        claude_output_tokens: None,
        claude_cache_creation_input_tokens: None,
        claude_cache_read_input_tokens: None,
        gemini_prompt_token_count: usage.prompt_token_count,
        gemini_candidates_token_count: usage.candidates_token_count,
        gemini_total_token_count: usage.total_token_count,
        gemini_cached_content_token_count: usage.cached_content_token_count,
    }
}

pub(crate) fn build_openai_response_usage_record(
    provider: ProviderKind,
    response_id: &str,
    created_at: i64,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: ResponseUsage,
) -> UsageRecord {
    UsageRecord {
        provider,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("openai".to_string()),
        request_id: Some(response_id.to_string()),
        created_at,
        oa_resp_input_tokens: Some(usage.input_tokens),
        oa_resp_output_tokens: Some(usage.output_tokens),
        oa_resp_total_tokens: Some(usage.total_tokens),
        oa_resp_input_tokens_details_cached_tokens: Some(usage.input_tokens_details.cached_tokens),
        oa_resp_output_tokens_details_reasoning_tokens: Some(
            usage.output_tokens_details.reasoning_tokens,
        ),
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: None,
        oa_chat_completion_tokens: None,
        oa_chat_total_tokens: None,
        oa_chat_prompt_tokens_details_cached_tokens: None,
        oa_chat_completion_tokens_details_reasoning_tokens: None,
        oa_chat_completion_tokens_details_audio_tokens: None,
        claude_input_tokens: None,
        claude_output_tokens: None,
        claude_cache_creation_input_tokens: None,
        claude_cache_read_input_tokens: None,
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}

pub(crate) fn ensure_openai_stream_usage(req: &mut CreateChatCompletionRequest) {
    if req.stream != Some(true) {
        return;
    }
    match req.stream_options.as_mut() {
        Some(options) => {
            if options.include_usage.is_none() {
                options.include_usage = Some(true);
            }
        }
        None => {
            req.stream_options = Some(ChatCompletionStreamOptions {
                include_usage: Some(true),
                include_obfuscation: None,
            });
        }
    }
}
