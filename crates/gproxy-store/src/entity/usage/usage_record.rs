//! Per-request usage summary. Identity fields are historical references, without configuration FKs.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_records")]
pub struct Model {
    /// The first three columns form the `by_user` key: an index over
    /// `(user_id, started_at_ms, request_id)`, the order a user's usage is
    /// paged and scanned in. It replaces the single-column index on `user_id`.
    /// Unique only because the entity macro can express a composite index no
    /// other way (the trailing primary key makes it so), which also means a
    /// column belongs to one such key at most: the key and model filters
    /// cannot get one over `started_at_ms` as well, and use their own
    /// single-column indexes.
    #[sea_orm(unique_key = "by_user")]
    pub user_id: Option<String>,
    #[sea_orm(indexed, unique_key = "by_user")]
    pub started_at_ms: i64,
    /// Downstream HTTP exchange or WS turn ID, never the whole WS connection.
    /// Logical reference to CaptureRecord.id; usage and log retention are independent.
    /// Shared upstream usage must be allocated explicitly, not summed per link.
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_user")]
    pub request_id: String,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    #[sea_orm(indexed)]
    pub model: String,
    pub operation: String,
    pub metrics: Json,
    /// For subscription requests, the settled charge is denominated in USD.
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub cost: Option<FixedDecimal>,
    /// Indexed for retention, which deletes the oldest-ended rows first.
    #[sea_orm(indexed)]
    pub ended_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
