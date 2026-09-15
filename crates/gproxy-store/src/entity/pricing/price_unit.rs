//! Quantity units; the rate denominator determines per-million/per-minute prices.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum PriceUnit {
    #[sea_orm(string_value = "token")]
    Token,
    /// Requests, searches, tool calls, sessions, images or videos, per metric.
    #[sea_orm(string_value = "count")]
    Count,
    #[sea_orm(string_value = "second")]
    Second,
    #[sea_orm(string_value = "character")]
    Character,
}
