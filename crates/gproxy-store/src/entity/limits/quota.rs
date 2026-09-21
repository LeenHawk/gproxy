//! Configured budgets, separate from historical consumption windows.
//!
//! A budget belongs to one owner, `(owner_kind, owner_id)`. Kinds are
//! host-defined strings: the host decides what levels of its organisation a
//! budget can sit at and passes, per request, the chain of owners the request
//! spends for; core matches kinds verbatim (no normalisation, no hierarchy).
//! Suggested kinds: `user`, `api_key`, `subscription`, `pool`, `team`, `org`.
//! There is no foreign key from a budget to its owner: the host write layer
//! owns referential consistency and cleans up budgets when an owner goes away.
//!
//! Pool budgets measure actual upstream work; subscription budgets measure
//! allocated downstream consumption. They are not upstream quota observations.
//! Subscription allocations and their pool monetary budgets use metric = cost,
//! unit = USD. Other owner kinds may use other metrics/units. The future
//! write layer must enforce the subscription denomination contract.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "quotas")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// Host-defined owner kind, matched verbatim. Both owner columns are
    /// indexed on their own; the entity macro offers no composite non-unique
    /// index, and a lookup is by kind and id together anyway.
    #[sea_orm(indexed)]
    pub owner_kind: String,
    #[sea_orm(indexed)]
    pub owner_id: String,
    /// Stable window key used by subscription views, e.g. primary/secondary.
    pub window_key: String,
    pub metric: String,
    pub unit: String,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub limit_value: FixedDecimal,
    pub period: String,
    pub period_seconds: Option<i64>,
    /// Fixed-window anchor; subscription issuance defaults this to starts_at_ms.
    /// Calendar windows use UTC; total has no reset. Sliding windows are not implied.
    pub anchor_at_ms: Option<i64>,
    pub model_pattern: Option<String>,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
}

impl ActiveModelBehavior for ActiveModel {}
