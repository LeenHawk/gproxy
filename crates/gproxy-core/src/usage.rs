//! Normalized usage and settlement: what the funnel produces.

use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Provider-independent usage for one request. First-class token fields
/// stay deliberately few; everything else is dimensional — a new measure
/// is an entry in `metrics`, priced by a rate rule, not a new column
/// (a first-class column cost v2 34 files).
#[derive(Debug, Clone, Default)]
pub struct NormalizedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    /// Quantities: `"audio_seconds"`, `"video_seconds"`, `"image_output"`...
    pub metrics: BTreeMap<String, Decimal>,
    /// Qualifiers that select pricing variants: `"resolution"`, `"tier"`...
    pub dimensions: BTreeMap<String, String>,
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
