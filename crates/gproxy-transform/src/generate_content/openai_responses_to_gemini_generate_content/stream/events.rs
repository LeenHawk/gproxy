use gproxy_protocol::openai;

use crate::TransformError;

pub(super) fn emit(
    event: openai::KnownResponseStreamEvent,
) -> Result<openai::ResponseStreamEvent, TransformError> {
    Ok(openai::ResponseStreamEvent::Known(Box::new(event)))
}

pub(super) fn message_item(
    item: &super::Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(crate::wire!(
        openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: item.id.clone(),
            role: openai::ResponseOutputMessageRole::Assistant,
            content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                message_part(item),
            )],
            status,
            phase: None,
            rest: Default::default(),
        }
    )))
}

pub(super) fn message_part(item: &super::Item) -> openai::ResponseOutputText {
    crate::wire!(openai::ResponseOutputText {
        type_: openai::ResponseOutputTextType::OutputText,
        annotations: Vec::new(),
        logprobs: None,
        text: item.text.clone(),
        rest: Default::default(),
    })
}

pub(super) fn reasoning_item(
    item: &super::Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id: Some(item.id.clone()),
        summary: Vec::new(),
        content: Some(vec![crate::wire!(openai::ResponseReasoningTextPart {
            text: item.text.clone(),
            type_: openai::ResponseReasoningTextType::ReasoningText,
            rest: Default::default(),
        })]),
        encrypted_content: item.signature.clone(),
        status: Some(status),
        rest: Default::default(),
    }))
}
