//! Provider-independent usage. Moved down from the core: extraction happens
//! in channels, so the type lives at the contract layer.

use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaCapabilities {
    pub probe: bool,
    pub reset: bool,
}

impl QuotaCapabilities {
    pub const SUBSCRIPTION: Self = Self {
        probe: true,
        reset: false,
    };
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "models", rename_all = "snake_case")]
pub enum QuotaScope {
    All,
    Models(Vec<String>),
    #[default]
    Unknown,
}

impl QuotaScope {
    pub fn includes(&self, model: &str) -> bool {
        match self {
            Self::All => true,
            Self::Models(models) => models.iter().any(|allowed| allowed == model),
            Self::Unknown => false,
        }
    }
}

/// Usage for one exchange. First-class token fields stay deliberately few;
/// everything else is dimensional — a new measure is an entry in `metrics`
/// priced by a data-driven rate rule, not a new column (a first-class
/// column cost v2 34 files).
#[derive(Debug, Clone, Default)]
pub struct NormalizedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    /// Quantities: `"audio_seconds"`, `"video_seconds"`, `"image_output"`...
    pub metrics: BTreeMap<String, Decimal>,
    /// Qualifiers that select pricing variants: `"resolution"`, `"tier"`...
    pub dimensions: BTreeMap<String, String>,
    /// Per-model attempts replace top-level counts for billing, never add to them.
    pub attempts: Vec<UsageAttempt>,
}

#[derive(Debug, Clone)]
pub struct UsageAttempt {
    pub model: String,
    pub usage: Box<NormalizedUsage>,
    pub billable: bool,
    pub estimated: bool,
    pub started_at_ms: Option<i64>,
}

/// One upstream quota-window reading riding a response. A channel reports
/// only what the wire declared — boundaries are unix seconds from the
/// upstream, never inferred locally.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaObservation {
    pub unit: Option<String>,
    pub reset_behavior: QuotaResetBehavior,
    pub scope: QuotaScope,
    pub sample: Option<QuotaSample>,
    pub window_key: String,
    /// Upstream display name for the limit (e.g. codex `limit_name`), when
    /// the wire carries one beside the stable key.
    pub label: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
    pub used_percent: Option<Decimal>,
    pub upstream_used: Option<Decimal>,
    pub upstream_limit: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuotaSample {
    #[serde(default)]
    pub source: QuotaSampleSource,
    pub started_at_ms: i64,
    pub received_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaSampleSource {
    Response,
    Probe,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaResetBehavior {
    Periodic,
    Recovering,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaResetCredits {
    pub available_count: i64,
    /// Soonest expiry among the credits, unix seconds — only the dedicated
    /// credits endpoint reports per-credit expiry; the usage summary does not.
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaResetOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaResetResult {
    pub outcome: QuotaResetOutcome,
    pub windows_reset: Option<i64>,
}
