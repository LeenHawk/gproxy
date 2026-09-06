use rust_decimal::Decimal;
use serde_json::Value;

use super::state::StateError;
use crate::BoxFuture;
use crate::wire::{MaybeSend, MaybeSync};

/// Who is asking (read-only).
#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub oauth_access_digest: Option<[u8; 32]>,
    pub user_id: i64,
    pub user_key_id: i64,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
}

/// The provider this surface serves (read-only).
pub struct ProviderView<'a> {
    pub id: i64,
    pub name: &'a str,
    pub settings: &'a Value,
}

/// Read-only local aggregates plus the selected credential's observed quota windows.
pub trait UsageView: MaybeSend + MaybeSync {
    fn window<'a>(&'a self, since_unix: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>>;

    fn quota_windows<'a>(&'a self) -> BoxFuture<'a, Result<Vec<QuotaWindow>, StateError>>;
}

#[derive(Debug, Clone, Default)]
pub struct UsageWindow {
    pub cost: Decimal,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct QuotaWindow {
    pub key: String,
    pub period_start: Option<i64>,
    pub reset_at: Option<i64>,
    pub used_percent: Option<Decimal>,
    pub upstream_used: Option<Decimal>,
    pub upstream_limit: Option<Decimal>,
}
