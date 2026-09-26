//! Upstream quota cycles of a credential: one row per period of one window,
//! updated in place while the period runs and closed when it ends.
//!
//! This is the "current state" table. `credential_quota_cycles` is the raw
//! observation log behind it, despite its name; an observation names the
//! cycle it fell in through `credential_quota_cycles.credential_cycle_id`
//! (not `cycle_id`, which blocks already use for the observation row itself).
//!
//! `credential_id` is a historical reference, kept after the credential is
//! deleted, like the observation log's.
//!
//! At most one cycle per `(credential_id, window_id)` is open at a time. The
//! guarantee is `open_key`: it holds `credential_id` and `window_id` while the
//! cycle is open and is cleared when it closes, and it is unique. SQLite,
//! Postgres and MySQL all admit any number of NULLs in a unique index, so the
//! closed history never collides, and two instances racing to open the same
//! window meet on that index instead of both succeeding. A partial unique index
//! would say the same thing more directly, and MySQL has none.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_cycles")]
pub struct Model {
    /// The first three columns form the `by_credential` key, which is unique
    /// only because the entity macro can express a composite index no other
    /// way; the trailing primary key makes it trivially so. What it is for is
    /// its `(credential_id, closed_at_ms)` prefix: "the open cycles of this
    /// credential" is read on every cache miss of the settlement path.
    #[sea_orm(unique_key = "by_credential")]
    pub credential_id: String,
    /// None while the cycle is open.
    #[sea_orm(unique_key = "by_credential")]
    pub closed_at_ms: Option<i64>,
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_credential")]
    pub id: String,
    /// `<len>:<credential_id>:<window_id>` while open, None once closed.
    #[sea_orm(unique)]
    pub open_key: Option<String>,
    /// The observed `QuotaEntry.id`; before any observation, the declared
    /// `QuotaDimension.id`; `month` for a credential that declares no window.
    pub window_id: String,
    /// The declared dimension (`QuotaDimension.id`) behind the window. None
    /// only for the `month` cycle of a credential without declarations.
    pub dimension_id: Option<String>,
    /// Channel `QuotaScope` JSON the cycle accrues for: the declared scope,
    /// or the observation's where the declaration leaves it `unknown`.
    pub scope: Json,
    pub starts_at_ms: i64,
    /// None for a `Total` window, which never ends.
    pub ends_at_ms: Option<i64>,
    pub boundary: CycleBoundary,
    pub opened_by: CycleOpening,
    /// USD settled against this cycle so far. Only requests that went
    /// through this deployment; overlapping windows each carry their own sum.
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub cost_usd: FixedDecimal,
    /// The latest upstream reading, taken as one set at `sample_at_ms`:
    /// percent, used and limit as reported, and `cost_usd` at that instant.
    /// An estimate divides within one set only, never across two.
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub sample_used_percent: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub sample_used: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub sample_limit: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub sample_cost_usd: Option<FixedDecimal>,
    /// None until the first observation lands on the cycle.
    pub sample_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}

/// Where a cycle's boundaries come from.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    DeriveActiveEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum CycleBoundary {
    /// Derived here from the declared window; a guess until observed.
    #[default]
    #[sea_orm(string_value = "local")]
    Local,
    /// Reported by the upstream.
    #[sea_orm(string_value = "observed")]
    Observed,
}

/// Why a cycle was opened.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    DeriveActiveEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum CycleOpening {
    /// The window's first charge, or its first started observation.
    #[default]
    #[sea_orm(string_value = "first_use")]
    FirstUse,
    /// The previous cycle of the window ran to its end.
    #[sea_orm(string_value = "rollover")]
    Rollover,
    /// The upstream reset the window before its end.
    #[sea_orm(string_value = "server_reset")]
    ServerReset,
    /// This deployment redeemed a reset credit.
    #[sea_orm(string_value = "manual_reset")]
    ManualReset,
}

/// The `open_key` of an open cycle. Length-prefixed so that no pair of ids
/// can spell another pair's key.
pub fn open_key(credential_id: &str, window_id: &str) -> String {
    format!("{}:{credential_id}:{window_id}", credential_id.len())
}
