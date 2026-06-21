use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::claude_response_blocks_to_chat_message;

pub fn response(
    input: claude::CreateMessageResponseBody,
    _: &TransformContext,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    Ok(openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![openai::ChatCompletionChoice {
            finish_reason: claude_stop_reason_to_chat(input.stop_reason),
            index: 0,
            logprobs: None,
            message: claude_response_blocks_to_chat_message(input.content),
            extra: Default::default(),
        }],
        created: 0,
        model: common::claude_model_string(input.model).into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier: claude_usage_service_tier_to_openai(input.usage.service_tier.as_ref()),
        system_fingerprint: None,
        usage: Some(common::claude_usage_to_completion(input.usage)),
        extra: Default::default(),
    })
}

fn claude_stop_reason_to_chat(reason: claude::StopReason) -> openai::ChatFinishReason {
    match reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => {
            openai::ChatFinishReason::Length
        }
        claude::StopReason::Known(claude::StopReasonKnown::ToolUse) => {
            openai::ChatFinishReason::ToolCalls
        }
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
            openai::ChatFinishReason::ContentFilter
        }
        claude::StopReason::Known(
            claude::StopReasonKnown::EndTurn
            | claude::StopReasonKnown::StopSequence
            | claude::StopReasonKnown::PauseTurn
            | claude::StopReasonKnown::Compaction,
        )
        | claude::StopReason::Unknown(_) => openai::ChatFinishReason::Stop,
    }
}

fn claude_usage_service_tier_to_openai(
    tier: Option<&claude::UsageServiceTier>,
) -> Option<openai::ServiceTier> {
    match tier? {
        claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Priority) => {
            Some(openai::ServiceTier::Priority)
        }
        claude::UsageServiceTier::Known(
            claude::UsageServiceTierKnown::Standard | claude::UsageServiceTierKnown::Batch,
        )
        | claude::UsageServiceTier::Unknown(_) => Some(openai::ServiceTier::Default),
    }
}
