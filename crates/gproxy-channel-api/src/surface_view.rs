use rust_decimal::Decimal;
use serde_json::Value;

use crate::BoxFuture;
use crate::surface_state::StateError;
use crate::wire::{MaybeSend, MaybeSync};

/// Who is asking (read-only).
#[derive(Debug, Clone)]
pub struct CallerIdentity {
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

/// Read-only local usage aggregates, scoped to (caller, provider).
pub trait UsageView: MaybeSend + MaybeSync {
    fn window<'a>(&'a self, since_unix: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>>;
}

#[derive(Debug, Clone, Default)]
pub struct UsageWindow {
    pub cost: Decimal,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
