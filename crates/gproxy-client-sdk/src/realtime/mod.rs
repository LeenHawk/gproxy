//! Bidirectional OpenAI Realtime and Gemini Live sessions over WebSocket.
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
mod collect;
mod config;
pub mod events;
pub mod gemini;
pub mod openai;
mod queue;
mod sender;
mod session;
mod stream;
pub use collect::RealtimeCollector;
pub use config::*;
pub use events::*;
pub use sender::RealtimeSender;
pub use session::RealtimeSession;
pub use stream::RealtimeStream;

pub(crate) fn warn(warnings: &mut Vec<Warning>, field: &str, message: &str) {
    warnings.push(Warning::new(WarningKind::Unsupported, field, message));
}
pub(crate) fn override_warning(warnings: &mut Vec<Warning>, field: &str) {
    warnings.push(Warning::new(
        WarningKind::Compatibility,
        field,
        "Native options override the portable setting",
    ));
}
pub(crate) fn json(value: impl Serialize) -> Result<Value> {
    serde_json::to_value(value).map_err(Error::invalid)
}

/// Typed native messages bypass portable operation mapping. Raw preserves future
/// protocol messages; callers own their session and collector implications.
#[derive(Debug, Clone, Copy)]
pub enum NativeRealtimeMessage<'a> {
    Openai(&'a protocol::openai::realtime::RealtimeClientEvent),
    Gemini(&'a protocol::gemini::live::LiveClientMessage),
    Raw(&'a Value),
}
