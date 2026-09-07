use gproxy_protocol::{claude, openai};

pub(crate) fn refusal_text(
    reason: &claude::StopReason,
    details: Option<&claude::StopDetails>,
) -> Option<String> {
    if !matches!(
        reason,
        claude::StopReason::Known(claude::StopReasonKnown::Refusal)
    ) {
        return None;
    }
    let explanation = match details {
        Some(claude::StopDetails::Refusal(details)) => details.explanation.as_deref(),
        Some(claude::StopDetails::Unknown(details)) => details
            .get("explanation")
            .and_then(serde_json::Value::as_str),
        _ => None,
    };
    Some(
        explanation
            .unwrap_or("The upstream model refused this request.")
            .into(),
    )
}

pub(crate) fn refusal_item(id: String, text: String) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(crate::wire!(
        openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id,
            role: openai::ResponseOutputMessageRole::Assistant,
            content: vec![openai::ResponseMessageOutputContentPart::Refusal(
                crate::wire!(openai::ResponseRefusal {
                    type_: openai::ResponseRefusalType::Refusal,
                    refusal: text,
                    rest: Default::default(),
                })
            )],
            status: openai::ResponseItemLifecycleStatus::Completed,
            phase: None,
            rest: Default::default(),
        }
    )))
}

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
