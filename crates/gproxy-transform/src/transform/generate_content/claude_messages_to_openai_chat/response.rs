use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::claude_response_blocks_to_chat_message;

pub fn response(
    input: claude::CreateMessageResponseBody,
    _: &TransformContext,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    let service_tier = common::claude_usage_to_openai_service_tier(&input.usage);
    let mut extra = Default::default();
    crate::transform::common::preserve_claude_input_transformations(
        &mut extra,
        input.input_transformations,
    );
    Ok(crate::protocol::wire!(openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![crate::protocol::wire!(openai::ChatCompletionChoice {
            finish_reason: claude_stop_reason_to_chat(input.stop_reason),
            index: 0,
            logprobs: None,
            message: claude_response_blocks_to_chat_message(input.content),
            extra: Default::default(),
        })],
        created: 0,
        model: common::claude_model_string(input.model).into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier,
        system_fingerprint: None,
        usage: Some(common::claude_usage_to_completion(input.usage)),
        extra,
    }))
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
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}
