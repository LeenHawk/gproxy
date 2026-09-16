//! Account quota queries and observations, independent of per-call token usage.

use gproxy_protocol::OperationKey;
use http::{HeaderMap, StatusCode};
use rust_decimal::Decimal;

use super::{ChannelError, CredentialContext, OperationFuture};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum QuotaScope {
    All,
    Models(Vec<String>),
    /// Exact prefix or prefix followed by a hyphen, e.g. a Claude model family.
    ModelPrefixes(Vec<String>),
    #[default]
    Unknown,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaValue {
    Window(QuotaAllowance),
    RateLimit(QuotaAllowance),
    Budget(QuotaAllowance),
    Balance(QuotaBalance),
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
pub struct QuotaResetCredits {
    pub available_count: u64,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaResetOutcome {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaResetResult {
    pub outcome: QuotaResetOutcome,
    pub windows_reset: Option<u64>,
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
        redeem_request_id: &'a str,
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
