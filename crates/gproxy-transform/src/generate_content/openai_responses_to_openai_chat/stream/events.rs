use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::SseFrame;

use super::{Tool, ToolKind};

pub(super) fn emit(event: openai::KnownResponseStreamEvent) -> Result<Bytes, TransformError> {
    SseFrame::typed(
        Some(event.event_name()),
        &openai::ResponseStreamEvent::Known(Box::new(event)),
    )
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
