use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::SseFrame;

pub(super) fn event(
    type_: openai::ResponseStreamEventTypeKnown,
) -> openai::KnownResponseStreamEvent {
    openai::KnownResponseStreamEvent {
        type_,
        sequence_number: None,
        response: None,
        item: None,
        output_index: None,
        content_index: None,
        item_id: None,
        part: None,
        delta: None,
        logprobs: None,
        text: None,
        annotation: None,
        annotation_index: None,
        arguments: None,
        name: None,
        input: None,
        refusal: None,
        summary_index: None,
        partial_image_b64: None,
        partial_image_index: None,
        code: None,
        message: None,
        param: None,
        reasoning_part: None,
        rest: Default::default(),
    }
}

pub(super) fn emit(event: openai::KnownResponseStreamEvent) -> Result<Bytes, TransformError> {
    let name = event.type_.as_str().to_owned();
    SseFrame::typed(
        Some(&name),
        &openai::ResponseStreamEvent::Known(Box::new(event)),
    )
}

pub(super) fn item_id(item: &openai::ResponseItem, index: u32) -> String {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            message.id.clone()
        }
        openai::ResponseItem::Typed(item) => match item.as_ref() {
            openai::TypedResponseItem::FunctionCall { id, .. }
            | openai::TypedResponseItem::CustomToolCall { id, .. }
            | openai::TypedResponseItem::ShellCall { id, .. }
            | openai::TypedResponseItem::ApplyPatchCall { id, .. }
            | openai::TypedResponseItem::ShellCallOutput { id, .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { id, .. }
            | openai::TypedResponseItem::Reasoning { id, .. } => id.clone(),
            openai::TypedResponseItem::LocalShellCall { id, .. }
            | openai::TypedResponseItem::LocalShellCallOutput { id, .. } => Some(id.clone()),
            _ => None,
        },
        _ => None,
    }
    .unwrap_or_else(|| format!("item_{index}"))
}

pub(super) fn message_item(
    item: &super::Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
        openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: Some(item.id.clone()),
            role: openai::ResponseOutputMessageRole::Assistant,
            content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                message_part(item),
            )],
            status,
            phase: None,
            rest: item.rest.clone(),
        },
    ))
}

pub(super) fn message_part(item: &super::Item) -> openai::ResponseOutputText {
    openai::ResponseOutputText {
        type_: openai::ResponseOutputTextType::OutputText,
        annotations: Vec::new(),
        logprobs: None,
        text: item.text.clone(),
        rest: Default::default(),
    }
}

pub(super) fn reasoning_item(
    item: &super::Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id: Some(item.id.clone()),
        summary: Vec::new(),
        content: Some(vec![openai::ResponseReasoningTextPart {
            text: item.text.clone(),
            type_: "reasoning_text".into(),
            rest: Default::default(),
        }]),
        encrypted_content: item.signature.clone(),
        status: Some(status),
        rest: item.rest.clone(),
    }))
}
