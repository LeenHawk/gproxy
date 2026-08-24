use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::SseFrame;

use super::{Item, Tool, ToolKind};

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

pub(super) fn message_item(
    item: &Item,
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

pub(super) fn message_part(item: &Item) -> openai::ResponseOutputText {
    openai::ResponseOutputText {
        type_: openai::ResponseOutputTextType::OutputText,
        annotations: Vec::new(),
        logprobs: (!item.logprobs.is_empty()).then(|| item.logprobs.clone()),
        text: item.text.clone(),
        rest: Default::default(),
    }
}

pub(super) fn reasoning_item(
    item: &Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id: Some(item.id.clone()),
        summary: Vec::new(),
        content: Some(vec![openai::ResponseReasoningTextPart {
            type_: openai::ResponseReasoningTextType::ReasoningText,
            text: item.text.clone(),
            rest: Default::default(),
        }]),
        encrypted_content: None,
        status: Some(status),
        rest: item.rest.clone(),
    }))
}

pub(super) fn tool_item(
    item: &Tool,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(match item.kind {
        ToolKind::Function => openai::TypedResponseItem::FunctionCall {
            arguments: item.arguments.clone(),
            call_id: item.id.clone(),
            name: item.name.clone(),
            id: Some(item.id.clone()),
            caller: None,
            namespace: None,
            status: Some(status),
            rest: item.rest.clone(),
        },
        ToolKind::Custom => openai::TypedResponseItem::CustomToolCall {
            call_id: item.id.clone(),
            input: item.arguments.clone(),
            name: item.name.clone(),
            id: Some(item.id.clone()),
            caller: None,
            namespace: None,
            rest: item.rest.clone(),
        },
    }))
}

pub(super) fn stream_logprob(value: openai::TokenLogprob) -> openai::StreamTokenLogprob {
    let mut rest = value.rest;
    if let Some(bytes) = value.bytes {
        rest.insert(
            "bytes".into(),
            serde_json::Value::Array(bytes.into_iter().map(serde_json::Value::from).collect()),
        );
    }
    openai::StreamTokenLogprob {
        token: value.token,
        logprob: value.logprob,
        top_logprobs: Some(
            value
                .top_logprobs
                .into_iter()
                .map(|top| {
                    let mut rest = top.rest;
                    if let Some(bytes) = top.bytes {
                        rest.insert(
                            "bytes".into(),
                            serde_json::Value::Array(
                                bytes.into_iter().map(serde_json::Value::from).collect(),
                            ),
                        );
                    }
                    openai::StreamTokenTopLogprob {
                        token: Some(top.token),
                        logprob: Some(top.logprob),
                        rest,
                    }
                })
                .collect(),
        ),
        rest,
    }
}
