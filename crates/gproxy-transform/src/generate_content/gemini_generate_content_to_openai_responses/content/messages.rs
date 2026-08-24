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
    rest: openai::Rest,
) {
    if parts.is_empty() {
        return;
    }
    if response {
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
            openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id: None,
                role: openai::ResponseOutputMessageRole::Assistant,
                content,
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                rest,
            }),
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
        openai::ResponseMessageItem::EasyInput(openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content: openai::ResponseEasyInputContent::Parts(content),
            phase: None,
            rest,
        }),
    ));
}
