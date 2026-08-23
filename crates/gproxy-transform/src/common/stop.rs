use gproxy_protocol::{claude, openai};

pub(crate) fn claude_to_chat(reason: &claude::StopReason) -> openai::ChatFinishReason {
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
        _ => openai::ChatFinishReason::Stop,
    }
}

pub(crate) fn chat_to_claude(reason: openai::ChatFinishReason) -> claude::StopReason {
    let known = match reason {
        openai::ChatFinishReason::Length => claude::StopReasonKnown::MaxTokens,
        openai::ChatFinishReason::ToolCalls | openai::ChatFinishReason::FunctionCall => {
            claude::StopReasonKnown::ToolUse
        }
        openai::ChatFinishReason::ContentFilter => claude::StopReasonKnown::Refusal,
        _ => claude::StopReasonKnown::EndTurn,
    };
    claude::StopReason::Known(known)
}
