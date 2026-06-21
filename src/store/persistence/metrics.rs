//! §15.3 metrics aggregates: persistence-derived snapshot rendered as Prometheus
//! text by [`crate::http::server::metrics`]. Everything here is computed by a
//! backend-side aggregate query (SUM/COUNT/bucketing) — never an in-memory
//! counter — so it works identically on native and the wasm edge, and is a
//! cross-instance-consistent global aggregate by construction.

use rust_decimal::Decimal;

/// Cumulative `le` (less-or-equal) upper bounds for the upstream-latency
/// histogram, in milliseconds. A `+Inf` bucket (= total count) is implied.
pub const LATENCY_BUCKETS_MS: &[i64] = &[50, 100, 250, 500, 1000, 2500, 5000, 10000];

/// One scope's quota gauge (`scope` ∈ org|team|user).
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaUsage {
    pub scope: String,
    pub scope_id: i64,
    pub total: Decimal,
    pub used: Decimal,
}

/// A full metrics snapshot derived from persistence (§15.3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricsAggregate {
    /// Cumulative settled requests (sum over all rollup buckets).
    pub requests_total: i64,
    pub input_tokens_total: i64,
    pub output_tokens_total: i64,
    /// Per-bucket cumulative counts aligned 1:1 with [`LATENCY_BUCKETS_MS`]:
    /// `latency_buckets[i]` = number of settled requests with `latency_ms <=
    /// LATENCY_BUCKETS_MS[i]`. Cumulative (Prometheus histogram convention).
    pub latency_buckets: Vec<i64>,
    /// Sum of all settled `latency_ms` (the histogram `_sum`).
    pub latency_sum_ms: i64,
    /// Total settled requests with a recorded latency (the histogram `_count`).
    pub latency_count: i64,
    /// Count of credential health rows grouped by `health_kind`.
    pub credential_health: Vec<(String, i64)>,
    /// Per-scope quota gauges.
    pub quota: Vec<QuotaUsage>,
}
