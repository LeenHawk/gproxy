use gproxy_protocol::{gemini, openai};

pub(super) enum MessagePart {
    Input(openai::ResponseInputContentPart),
    Output(openai::ResponseMessageOutputContentPart),
}

pub(super) fn flush(
    output: &mut Vec<openai::ResponseItem>,
    parts: &mut Vec<MessagePart>,
    role: Option<&gemini::ContentRole>,
    response: bool,
    next_message: &mut u32,
) {
    if parts.is_empty() {
        return;
    }
    if response {
        let id = format!("msg_gemini_{}", *next_message);
        *next_message = next_message.saturating_add(1);
        let content = std::mem::take(parts)
            .into_iter()
            .map(|part| match part {
                MessagePart::Output(part) => part,
                MessagePart::Input(_) => {
                    unreachable!("input part produced while converting a Gemini response")
                }
            })
            .collect();
        output.push(openai::ResponseItem::Message(
            openai::ResponseMessageItem::Output(crate::wire!(openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id,
                role: openai::ResponseOutputMessageRole::Assistant,
                content,
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                rest: Default::default(),
            })),
        ));
        return;
    }
    let role = match role {
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => {
            openai::ResponseEasyInputMessageRole::Assistant
        }
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
            openai::ResponseEasyInputMessageRole::System
        }
        _ => openai::ResponseEasyInputMessageRole::User,
    };
    let content = std::mem::take(parts)
        .into_iter()
        .map(|part| match part {
            MessagePart::Input(part) => part,
            MessagePart::Output(_) => {
                unreachable!("output part produced while converting a Gemini request")
            }
        })
        .collect();
    output.push(openai::ResponseItem::Message(
        openai::ResponseMessageItem::EasyInput(crate::wire!(
            openai::ResponseEasyInputMessageItem {
                type_: Some(openai::ResponseMessageItemType::Message),
                role,
                content: openai::ResponseEasyInputContent::Parts(content),
                phase: None,
                rest: Default::default(),
            }
        )),
    ));
}
