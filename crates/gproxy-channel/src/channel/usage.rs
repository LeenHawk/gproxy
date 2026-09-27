//! Per-call metering. Reading usage does not query account quotas or settle a bill.

use gproxy_protocol::{OperationKey, connection::WsFrame};
use http::{HeaderMap, StatusCode};

use super::ChannelError;

// The metering vocabulary is the protocol's, where the standard readers
// produce it; it stays reachable at this path for every existing user.
pub use gproxy_protocol::usage::{
    NormalizedUsage, ResponseUsage, TokenUsage, UsageAttempt, UsageCompleteness, UsageStreamEnd,
    UsageTransport,
};

pub struct ResponseView<'a> {
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
}

pub struct UsageContext<'a> {
    pub operation: OperationKey,
    /// Optional so metering does not itself force buffering a streamed request.
    pub request_body: Option<&'a [u8]>,
    pub response: ResponseView<'a>,
}

pub trait UsageExtractor: Send + Sync {
    /// None means the response contains no usage, rather than zero consumption.
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError>;
}

pub struct UsageStreamContext<'a> {
    pub operation: OperationKey,
    pub request_body: Option<&'a [u8]>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    pub transport: UsageTransport,
}

pub enum UsageFrame<'a> {
    /// Raw chunks, not complete SSE/JSON records. The observer handles framing.
    HttpChunk(&'a [u8]),
    WebSocket(&'a WsFrame),
}

/// Per-response observation state. It borrows chunks/frames and never rewrites
/// the delivered stream. Snapshots and finish are cumulative replacements, not
/// additive deltas. EOF alone does not establish complete provider usage.
pub trait UsageObserver: Send {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError>;
    fn snapshot(&self) -> Option<NormalizedUsage>;
    fn finish(
        self: Box<Self>,
        end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError>;
}

pub trait UsageStream: Send + Sync {
    /// Opt in to observing a channel-owned binary framing. Ordinary JSON
    /// bodies keep using UsageExtractor instead.
    fn accepts_unframed(&self, _headers: &HeaderMap) -> bool {
        false
    }

    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError>;
}
