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
    /// Independently identified upstream responses inside a long-lived call.
    /// Their sum is the aggregate above; the settlement layer may deduplicate
    /// these across connections before pricing. IDs are upstream identities.
    pub responses: Vec<ResponseUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseUsage {
    pub id: String,
    pub usage: Box<NormalizedUsage>,
}

impl NormalizedUsage {
    /// Sum reported response quantities. Missing values remain unknown rather
    /// than becoming invented zeros. Qualifiers survive only when all agree.
    pub fn aggregate<'a>(values: impl IntoIterator<Item = &'a NormalizedUsage>) -> Self {
        let mut total = Self {
            completeness: UsageCompleteness::Complete,
            ..Default::default()
        };
        let mut first = true;
        fn add(target: &mut Option<u64>, value: Option<u64>) {
            if let Some(v) = value {
                *target = Some(target.unwrap_or(0).saturating_add(v));
            }
        }
        for value in values {
            add(&mut total.tokens.input_tokens, value.tokens.input_tokens);
            add(&mut total.tokens.output_tokens, value.tokens.output_tokens);
            add(
                &mut total.tokens.cached_input_tokens,
                value.tokens.cached_input_tokens,
            );
            add(
                &mut total.tokens.reasoning_tokens,
                value.tokens.reasoning_tokens,
            );
            add(
                &mut total.tokens.cache_creation_5m_tokens,
                value.tokens.cache_creation_5m_tokens,
            );
            add(
                &mut total.tokens.cache_creation_30m_tokens,
                value.tokens.cache_creation_30m_tokens,
            );
            add(
                &mut total.tokens.cache_creation_1h_tokens,
                value.tokens.cache_creation_1h_tokens,
            );
            for (key, count) in &value.metrics {
                // Saturating: upstream-reported amounts, and a Decimal
                // overflow panics.
                let sum = total.metrics.entry(key.clone()).or_default();
                *sum = sum.saturating_add(*count);
            }
            if first {
                total.dimensions = value.dimensions.clone();
                total.actual_service_tier = value.actual_service_tier.clone();
                first = false;
            } else {
                total
                    .dimensions
                    .retain(|key, v| value.dimensions.get(key) == Some(v));
                if total.actual_service_tier != value.actual_service_tier {
                    total.actual_service_tier = None;
                }
            }
            if value.completeness != UsageCompleteness::Complete {
                total.completeness = UsageCompleteness::Partial;
            }
        }
        total
    }
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
