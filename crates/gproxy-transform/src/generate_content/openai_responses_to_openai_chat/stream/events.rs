use gproxy_protocol::openai;

use crate::TransformError;

use super::{Tool, ToolKind};

pub(super) fn emit(
    event: openai::KnownResponseStreamEvent,
) -> Result<openai::ResponseStreamEvent, TransformError> {
    Ok(openai::ResponseStreamEvent::Known(Box::new(event)))
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
            async_: None,
            status: Some(status),
            rest: Default::default(),
        },
        ToolKind::Custom => openai::TypedResponseItem::CustomToolCall {
            call_id: item.id.clone(),
            input: item.arguments.clone(),
            name: item.name.clone(),
            id: Some(item.id.clone()),
            caller: None,
            namespace: None,
            async_: None,
            rest: Default::default(),
        },
    }))
}

pub(super) fn stream_logprob(value: openai::TokenLogprob) -> openai::StreamTokenLogprob {
    crate::wire!(openai::StreamTokenLogprob {
        token: value.token,
        logprob: value.logprob,
        top_logprobs: Some(
            value
                .top_logprobs
                .into_iter()
                .map(|top| openai::StreamTokenTopLogprob {
                    token: Some(top.token),
                    logprob: Some(top.logprob),
                    rest: Default::default(),
                })
                .collect(),
        ),
        rest: Default::default(),
    })
}
