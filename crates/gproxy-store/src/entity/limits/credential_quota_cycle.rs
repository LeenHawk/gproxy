//! The upstream quota observation log: one row per persisted reading of one
//! window, despite the table's name (it predates `credential_cycles`, which
//! holds the cycles themselves). `id` is what a `QuotaExhausted` block calls
//! `cycle_id`; the cycle a reading fell in is `credential_cycle_id`.
//! credential_id remains a historical reference after credential deletion.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_quota_cycles")]
pub struct Model {
    /// The first three columns form the `by_credential` key: an index over
    /// `(credential_id, observed_at_ms)` for time-ranged reads of one
    /// credential, unique only because the entity macro can express a
    /// composite index no other way (the trailing primary key makes it so).
    /// It replaces the single-column index on `credential_id`.
    #[sea_orm(unique_key = "by_credential")]
    pub credential_id: String,
    /// Indexed on its own too, for retention's age cut across credentials.
    #[sea_orm(indexed, unique_key = "by_credential")]
    pub observed_at_ms: i64,
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_credential")]
    pub id: String,
    pub scope: Json,
    pub snapshot: Json,
    pub starts_at_ms: Option<i64>,
    pub resets_at_ms: Option<i64>,
    /// The `credential_cycles` row this reading belonged to; None for an
    /// observe-only window, or a reading of a window nobody has used yet.
    pub credential_cycle_id: Option<String>,
    /// That cycle's `cost_usd` when the reading was taken, so two readings
    /// of one cycle give the USD spent between them.
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub cycle_cost_usd: Option<FixedDecimal>,
}

impl ActiveModelBehavior for ActiveModel {}
