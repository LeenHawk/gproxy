use bytes::Bytes;
use gproxy_protocol::claude;

use crate::TransformError;
use crate::envelope::SseFrame;

pub(super) fn encode(event: claude::KnownStreamEvent) -> Result<Bytes, TransformError> {
    let name = event.event_name();
    SseFrame::typed(Some(name), &claude::StreamEvent::Known(Box::new(event)))
}

pub(super) fn start(
    id: String,
    model: String,
    rest: claude::JsonObject,
) -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::MessageStart {
        message: Box::new(claude::CreateMessageStartBody {
            id,
            type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
            role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
            content: Vec::new(),
            model: model.into(),
            stop_reason: None,
            stop_sequence: None,
            usage: None,
            input_transformations: None,
            rest: Default::default(),
        }),
        rest,
    }
}

pub(super) fn block_start(index: u64, block: claude::ContentBlock) -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::ContentBlockStart {
        index,
        content_block: Box::new(block),
        rest: Default::default(),
    }
}

pub(super) fn block_delta(index: u64, delta: claude::KnownEventDelta) -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::ContentBlockDelta {
        index,
        delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
        rest: Default::default(),
    }
}

pub(super) fn block_stop(index: u64) -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::ContentBlockStop {
        index,
        rest: Default::default(),
    }
}

pub(super) fn message_delta(
    stop_reason: Option<claude::StopReason>,
    stop_sequence: Option<String>,
    usage: Option<claude::Usage>,
    rest: claude::JsonObject,
) -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::MessageDelta {
        context_management: None,
        delta: Box::new(claude::MessageDelta {
            container: None,
            stop_reason,
            stop_sequence,
            stop_details: None,
            rest: Default::default(),
        }),
        input_transformations: None,
        usage: usage.map(Box::new),
        rest,
    }
}

pub(super) fn message_stop() -> claude::KnownStreamEvent {
    claude::KnownStreamEvent::MessageStop {
        rest: Default::default(),
    }
}
