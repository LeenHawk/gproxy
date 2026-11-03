use super::*;
pub(super) use protocol::openai::realtime as o;
mod audio;
mod config;
mod decode;
mod dispatch;
mod items;
pub(super) use audio::format_from_wire;
pub use config::{OpenaiRealtimeOptions, render};
pub(super) use decode::decode;
pub(super) use items::{history, text, tool_result};

pub(super) fn update(request: o::RealtimeSessionCreateRequest) -> o::SessionUpdateEvent {
    o::SessionUpdateEvent::builder(
        o::SessionUpdateEventSession::RealtimeSessionCreateRequest(request),
        o::SessionUpdateEventType::SessionUpdate,
    )
    .build()
}
