//! Per-request usage summary. Identity fields are historical references, without configuration FKs.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_records")]
pub struct Model {
    /// Downstream HTTP exchange or WS turn ID, never the whole WS connection.
    /// Logical reference to CaptureRecord.id; usage and log retention are independent.
    /// Shared upstream usage must be allocated explicitly, not summed per link.
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_id: String,
    #[sea_orm(indexed)]
    pub user_id: Option<String>,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    /// Historical allocation at request time; changing the key's subscription
    /// must not reattribute old usage. No configuration FK.
    #[sea_orm(indexed)]
    pub subscription_id: Option<String>,
    pub model: String,
    pub operation: String,
    pub metrics: Json,
    /// For subscription requests, the settled charge is denominated in USD.
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cost: Option<Decimal>,
    #[sea_orm(indexed)]
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
