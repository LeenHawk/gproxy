//! Account quota queries and observations, independent of per-call token usage.

use gproxy_protocol::{Operation, OperationKey};
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;
use std::borrow::Cow;

use super::{ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView};

/// Serialized as `"all"`, `{"models":[..]}`, `{"model_prefixes":[..]}` or
/// `"unknown"`; this is the JSON shape persisted by the host for scopes.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaScope {
    All,
    Models(Vec<String>),
    /// Exact prefix or prefix followed by a hyphen, e.g. a Claude model family.
    ModelPrefixes(Vec<String>),
    #[default]
    Unknown,
}

impl QuotaScope {
    /// Whether the scope positively covers `model`. `Unknown` never matches;
    /// the host decides separately how to treat an unknown-scope observation.
    pub fn matches(&self, model: &str) -> bool {
        match self {
            Self::All => true,
            Self::Models(models) => models.iter().any(|m| m == model),
            Self::ModelPrefixes(prefixes) => prefixes.iter().any(|prefix| {
                model == prefix
                    || model
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|rest| rest.starts_with('-'))
            }),
            Self::Unknown => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum QuotaSubject {
    Account,
    Organization,
    Project,
    Key,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum QuotaResetBehavior {
    Periodic,
    Recovering,
    #[default]
    Unknown,
}

/// None means unreported, not zero. Percent is on the 0..100 scale. Period
/// boundaries come from upstream; the base does not infer a start from a reset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuotaAllowance {
    pub used: Option<Decimal>,
    pub limit: Option<Decimal>,
    pub remaining: Option<Decimal>,
    pub used_percent: Option<Decimal>,
    pub unlimited: Option<bool>,
    pub unit: Option<String>,
    pub period_start_ms: Option<i64>,
    pub period_end_ms: Option<i64>,
    pub reset_behavior: QuotaResetBehavior,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuotaBalance {
    pub remaining: Option<Decimal>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuotaBreakdownRow {
    pub key: String,
    pub label: Option<String>,
    pub percent: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaValue {
    Window(QuotaAllowance),
    RateLimit(QuotaAllowance),
    Budget(QuotaAllowance),
    Balance(QuotaBalance),
    /// Composition of used quota, not an independently limited window.
    Breakdown(Vec<QuotaBreakdownRow>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaEntry {
    pub id: String,
    pub source_id: String,
    pub label: Option<String>,
    pub subject: QuotaSubject,
    pub model_scope: QuotaScope,
    pub value: QuotaValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaSnapshot {
    pub observed_at_ms: i64,
    pub entries: Vec<QuotaEntry>,
}

/// E.g. Codex /wham/usage or ClaudeCode /api/oauth/usage. An implementation
/// handles any provider-specific pagination through the assigned client.
pub trait QuotaQuery: Send + Sync {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaMetric {
    Requests,
    Tokens,
    Cost,
    /// A provider-specific unit named by the channel.
    Unit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaWindow {
    /// Resets a fixed interval after first use or after the last reset the
    /// upstream reports, e.g. Codex's 5h/7d windows.
    Rolling {
        seconds: i64,
    },
    /// Fixed windows aligned to an anchor; None anchors to credential creation.
    Fixed {
        seconds: i64,
        anchor_at_ms: Option<i64>,
    },
    CalendarDay,
    CalendarWeek,
    CalendarMonth,
    /// Never resets, e.g. a prepaid balance.
    Total,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaTracking {
    /// The upstream reports it through headers, a query endpoint or exhaustion
    /// replies; observed entries match it through `QuotaModel::classify`.
    Reported,
    /// The upstream reports nothing; the host counts consumption itself. A
    /// counted dimension needs a known `limit`.
    Counted,
}

/// One quota dimension a credential has, declared by the channel from the
/// credential's auth kind and plan fields. This is the shape, not a reading:
/// values arrive later as `QuotaEntry`s whose `id` and `source_id` equal `id`.
/// `Unknown` scope declares a window whose models only an observation names;
/// `QuotaModel::classify` then supplies the scope per entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaDimension {
    /// Stable key, e.g. `primary`, `secondary`, `spark`, `seven_day_sonnet`.
    pub id: String,
    pub label: Option<String>,
    pub scope: QuotaScope,
    /// None applies to every operation.
    pub operations: Option<Vec<Operation>>,
    pub metric: QuotaMetric,
    pub window: QuotaWindow,
    /// Known statically from the plan; None means the upstream reports it.
    pub limit: Option<Decimal>,
    pub tracking: QuotaTracking,
    /// Whether an exhausted reading blocks the credential until the window
    /// resets. False keeps the credential routable and leaves the verdict to
    /// the upstream's replies, e.g. Claude Code's low-priority mode, which
    /// the server serves past a full five-hour window.
    pub blocking: bool,
}

/// Which quota dimensions this channel's credentials have. Synchronous and
/// pure: plan information the channel learns during login or refresh must be
/// written into the credential's metadata by the host, not fetched here.
/// An empty list means the credential has no modelled quota; the host then
/// relies on exhaustion replies alone.
///
/// The contract every channel keeps: each window that should accrue cost is
/// a declared dimension; the header and query paths name the same window with
/// the same entry id; an entry that `classify` does not place is observe-only
/// (recorded and shown, never charged). Channel tests pin both lists against
/// captured upstream replies.
pub trait QuotaModel: Send + Sync {
    fn dimensions(
        &self,
        provider: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension>;

    /// The dimension an observed entry reports on, as the host should apply
    /// it; None makes the entry observe-only. `declared` is what `dimensions`
    /// returned for this credential, plus any host-added dimensions. The
    /// default matches `source_id` by id; a channel whose windows are only
    /// known at runtime overrides it to fill in the scope from the entry.
    fn classify<'d>(
        &self,
        declared: &'d [QuotaDimension],
        entry: &QuotaEntry,
    ) -> Option<Cow<'d, QuotaDimension>> {
        classify_by_id(declared, entry)
    }
}

/// `QuotaModel::classify`'s default, also what the host applies to a channel
/// that has no `QuotaModel`.
pub fn classify_by_id<'d>(
    declared: &'d [QuotaDimension],
    entry: &QuotaEntry,
) -> Option<Cow<'d, QuotaDimension>> {
    declared
        .iter()
        .find(|dimension| dimension.id == entry.source_id)
        .map(Cow::Borrowed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaResetCredits {
    /// One expiry per available card, earliest first; None means unreported.
    pub credit_expirations_ms: Vec<Option<i64>>,
    pub options: Vec<QuotaResetOption>,
    /// None means the server did not disclose a count (for example CLI ineligibility).
    pub available_count: Option<u64>,
    pub expires_at_ms: Option<i64>,
}

/// A specific server-authorized reset choice; never chosen implicitly by the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaResetOption {
    pub program: String,
    pub grant_id: Option<String>,
    pub label: Option<String>,
    pub available_count: Option<u64>,
    pub expires_at_ms: Option<i64>,
    pub next_available_at_ms: Option<i64>,
    pub usable: bool,
    pub ineligible_reason: Option<String>,
    pub clears: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct QuotaResetRequest<'a> {
    pub redeem_request_id: &'a str,
    pub program: Option<&'a str>,
    pub grant_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaResetOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
    Ineligible,
    Unavailable,
    Cooldown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaResetResult {
    pub reason: Option<String>,
    pub outcome: QuotaResetOutcome,
    pub windows_reset: Option<u64>,
    /// Dimension ids the redeemed option reopens, as its `QuotaResetOption`
    /// listed them; empty when the channel does not say, which the host
    /// reads as every window.
    pub clears: Vec<String>,
}

/// Independent from quota querying: ClaudeCode need not implement resets just
/// because it can report usage windows. No automatic credit consumption.
pub trait QuotaReset: Send + Sync {
    fn credits<'a>(
        &'a self,
        context: CredentialContext<'a>,
    ) -> OperationFuture<'a, QuotaResetCredits>;

    fn reset<'a>(
        &'a self,
        context: CredentialContext<'a>,
        request: QuotaResetRequest<'a>,
    ) -> OperationFuture<'a, QuotaResetResult>;
}

pub struct QuotaHeaderContext<'a> {
    pub operation: OperationKey,
    pub upstream_model: &'a str,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
}

/// Observe response headers, e.g. Codex's x-codex-* and feature-specific limit
/// families. Empty means no quota data was reported. The host timestamps receipt.
pub trait QuotaHeaders: Send + Sync {
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError>;
}
