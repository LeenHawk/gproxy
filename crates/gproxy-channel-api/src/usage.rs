//! Per-credential upstream usage / quota snapshot (§17). OAuth subscription
//! channels expose a usage endpoint that reports the account's rate-limit
//! windows and (where applicable) credit balance for a single credential. Each
//! channel parses its provider-specific response into this shared shape; the
//! raw upstream JSON is retained in [`UsageSnapshot::raw`] so the admin UI can
//! surface fields this normalization does not model.
//!
//! The fetch is driven exactly like a credential refresh (resolve the
//! credential's pooled client → send [`Channel::prepare_usage_request`] →
//! [`Channel::parse_usage`]); the host owns transport and persistence.
//!
//! [`Channel::prepare_usage_request`]: crate::Channel::prepare_usage_request
//! [`Channel::parse_usage`]: crate::Channel::parse_usage

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Normalized usage/quota snapshot for one credential.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageSnapshot {
    /// Plan / subscription label when the provider reports one (`"pro"`,
    /// `"KIRO PRO+"`, `"business"`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Rate-limit / quota windows (5h + 7d, primary/secondary, per-model, per
    /// feature). Empty when the provider only reports credits.
    pub windows: Vec<UsageWindow>,
    /// Money / credit balance + overage, when the channel exposes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<UsageCredits>,
    /// Earned rate-limit reset credits, when the upstream exposes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_reset_credits: Option<RateLimitResetCredits>,
    /// The original upstream response JSON, for display / debugging.
    pub raw: Value,
}

/// A single rate-limit or quota window. Providers report usage either as a
/// percentage (`used_percent`) or as absolute counts (`used` / `limit`); a
/// window carries whichever the upstream gives. Reset time is kept verbatim as
/// an ISO-8601 string (`resets_at`) and/or unix seconds (`resets_at_unix`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageWindow {
    /// Window id (`"five_hour"`, `"seven_day"`, `"primary"`, a model id, …).
    pub name: String,
    /// Human-readable upstream label when `name` is generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    /// ISO-8601 reset timestamp, when the provider gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    /// Unix-seconds reset timestamp, when the provider gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at_unix: Option<i64>,
    /// Window length in seconds, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i64>,
}

/// Stable semantics for one provider-specific quota window.
///
/// [`UsageWindow`] intentionally stays close to the upstream response. This
/// descriptor supplies the extra identity and accounting semantics a host
/// needs to match the same window across refreshes and completed periods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageWindowDescriptor {
    /// Stable within one channel. Hosts that combine channels should namespace
    /// this key with [`Channel::id`](crate::Channel::id).
    pub key: String,
    /// Which locally recorded traffic is governed by this window.
    pub scope: UsageWindowScope,
    /// The upstream unit represented by `used`, `limit`, or `used_percent`.
    pub meter: UsageWindowMeter,
    /// Inclusive period start, when it can be established, in unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start_unix: Option<i64>,
    /// How the period boundary was established.
    pub boundary_source: UsageWindowBoundarySource,
    /// Whether the complete boundary is exact, derived, partial, or unknown.
    pub boundary_confidence: UsageWindowBoundaryConfidence,
}

impl UsageWindowDescriptor {
    /// Conservative descriptor for a normalized window. Channel adapters can
    /// refine its scope and meter through [`Channel::describe_usage_window`].
    ///
    /// [`Channel::describe_usage_window`]: crate::Channel::describe_usage_window
    pub fn from_window(window: &UsageWindow) -> Self {
        let period_start_unix = window
            .resets_at_unix
            .zip(window.window_seconds)
            .filter(|(_, seconds)| *seconds > 0)
            .map(|(reset, seconds)| reset.saturating_sub(seconds));
        let (boundary_source, boundary_confidence) = if period_start_unix.is_some() {
            (
                UsageWindowBoundarySource::ResetAndDuration,
                UsageWindowBoundaryConfidence::Exact,
            )
        } else if window.resets_at_unix.is_some() || window.resets_at.is_some() {
            (
                UsageWindowBoundarySource::ResetOnly,
                UsageWindowBoundaryConfidence::Partial,
            )
        } else {
            (
                UsageWindowBoundarySource::Unknown,
                UsageWindowBoundaryConfidence::Unknown,
            )
        };
        Self {
            key: window.name.clone(),
            scope: UsageWindowScope::Unknown,
            meter: UsageWindowMeter::Opaque,
            period_start_unix,
            boundary_source,
            boundary_confidence,
        }
    }

    pub fn scope(mut self, scope: UsageWindowScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn meter(mut self, meter: UsageWindowMeter) -> Self {
        self.meter = meter;
        self
    }

    pub fn period_start(
        mut self,
        unix: i64,
        source: UsageWindowBoundarySource,
        confidence: UsageWindowBoundaryConfidence,
    ) -> Self {
        self.period_start_unix = Some(unix);
        self.boundary_source = source;
        self.boundary_confidence = confidence;
        self
    }
}

/// Local-usage scope governed by an upstream quota window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsageWindowScope {
    All,
    Models { models: Vec<String> },
    Feature { feature: String },
    Unknown,
}

/// Upstream accounting unit for a quota window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowMeter {
    Tokens,
    Requests,
    Credits,
    Usd,
    Opaque,
}

/// Origin of the normalized period boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowBoundarySource {
    /// The upstream supplied an explicit start boundary.
    Upstream,
    /// The upstream supplied reset time and window duration.
    ResetAndDuration,
    /// The adapter derived the start from a documented/known window duration.
    KnownWindow,
    /// Only the reset/end boundary is known.
    ResetOnly,
    Unknown,
}

/// Confidence in the normalized period boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowBoundaryConfidence {
    Exact,
    Derived,
    Partial,
    Unknown,
}

impl UsageWindow {
    /// A percentage-based window (`used_percent` in \[0, 100\]).
    pub fn percent(name: impl Into<String>, used_percent: f64) -> Self {
        Self {
            name: name.into(),
            used_percent: Some(used_percent),
            ..Default::default()
        }
    }

    /// An absolute-count window (`used` / `limit`).
    pub fn amounts(name: impl Into<String>, used: f64, limit: f64) -> Self {
        Self {
            name: name.into(),
            used: Some(used),
            limit: Some(limit),
            ..Default::default()
        }
    }

    /// Attach an ISO-8601 reset timestamp.
    pub fn resets_iso(mut self, iso: impl Into<String>) -> Self {
        self.resets_at = Some(iso.into());
        self
    }

    /// Attach a unix-seconds reset timestamp.
    pub fn resets_unix(mut self, unix: i64) -> Self {
        self.resets_at_unix = Some(unix);
        self
    }

    /// Attach the window length in seconds.
    pub fn window_secs(mut self, seconds: i64) -> Self {
        self.window_seconds = Some(seconds);
        self
    }

    /// Attach a display label for generated / scoped windows.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Money / credit balance and on-demand overage, where the channel exposes it
/// (codex credits, claudecode `extra_usage`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageCredits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_credits: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unlimited: Option<bool>,
    /// Formatted balance string when the provider gives one (codex `balance`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    /// Credits consumed, normalized to the provider's display unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_credits: Option<f64>,
    /// Spending cap, normalized to the provider's display unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_limit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RateLimitResetCredits {
    pub available_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RateLimitResetCreditConsumeResponse {
    pub outcome: RateLimitResetCreditConsumeOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_reset: Option<i64>,
    pub raw: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitResetCreditConsumeOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}
