//! Settlement: what the funnel produces.

use rust_decimal::Decimal;

/// Provider-independent usage — defined in the channel contract (channels
/// extract it) and re-exported here.
pub use gproxy_channel_api::NormalizedUsage;

/// Cross-target admission estimate until a model tokenizer is available.
/// Counts UTF-8 scalar starts and charges one token per two characters.
pub fn estimate_input_tokens(body: &[u8]) -> u64 {
    utf8_chars(body).div_ceil(2)
}

pub(crate) fn utf8_chars(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .filter(|byte| **byte & 0b1100_0000 != 0b1000_0000)
        .count() as u64
}

/// Where the numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageSource {
    /// Reported by the upstream response.
    Upstream,
    /// Locally estimated (tokenizer ladder) because the upstream was silent.
    Estimated,
}

/// How the exchange ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Complete,
    /// Client hung up or the stream broke; usage may be partial.
    Interrupted,
}

/// The funnel's product: one settled exchange. Handed to the host's
/// `UsageSink`; the same struct reconciles quota pre-charges internally.
#[derive(Debug, Clone)]
pub struct Settlement {
    pub request_id: String,
    pub provider_id: i64,
    pub credential_id: crate::host::CredentialId,
    pub upstream_model: String,
    pub usage: NormalizedUsage,
    pub cost: Decimal,
    pub source: UsageSource,
    pub ended: Ended,
    pub latency_ms: u64,
}
