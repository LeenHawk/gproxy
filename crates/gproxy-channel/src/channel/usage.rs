//! Per-call metering. Reading usage does not query account quotas or settle a bill.

use std::collections::BTreeMap;

use gproxy_protocol::{
    OperationKey,
    connection::{StreamFraming, WsFrame},
};
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;

use super::ChannelError;

/// Reported counts only. None is unknown; Some(0) is an explicit zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// Ordinary input, excluding cache reads and cache creation.
    pub input_tokens: Option<u64>,
    /// Total output including reasoning; reasoning is a subset, not additive.
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_creation_5m_tokens: Option<u64>,
    pub cache_creation_30m_tokens: Option<u64>,
    pub cache_creation_1h_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UsageCompleteness {
    Complete,
    Partial,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedUsage {
    pub tokens: TokenUsage,
    /// Other reported quantities, e.g. audio_seconds, image_outputs, web_searches.
    /// Do not duplicate named token fields here; absence is not a measured zero.
    pub metrics: BTreeMap<String, Decimal>,
    /// Pricing qualifiers such as resolution, speed or inference_geo.
    pub dimensions: BTreeMap<String, String>,
    /// Actual upstream serving tier, which may differ from the requested tier.
    pub actual_service_tier: Option<String>,
    pub completeness: UsageCompleteness,
    /// When present, per-attempt usage replaces aggregate usage for billing.
    pub attempts: Vec<UsageAttempt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAttempt {
    pub model: String,
    pub usage: Box<NormalizedUsage>,
    /// None when the provider did not establish whether this attempt is charged.
    pub billable: Option<bool>,
    pub started_at_ms: Option<i64>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageTransport {
    Http { framing: Option<StreamFraming> },
    WebSocket,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageStreamEnd {
    Complete,
    Interrupted,
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
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError>;
}
