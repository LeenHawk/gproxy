use sea_query::{Alias, Expr, ExprTrait, Order, Query};

use crate::StoreError;
use crate::backend::Statement;

const COLUMNS: &[&str] = &[
    "request_id",
    "at",
    "provider_id",
    "operation",
    "upstream_model",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cost",
    "usage_source",
    "ended",
    "latency_ms",
];

pub(crate) fn recent_for_key(user_key_id: i64, limit: u64) -> Result<Statement, StoreError> {
    recent("user_key_id", user_key_id, limit)
}

pub(crate) fn recent_for_user(user_id: i64, limit: u64) -> Result<Statement, StoreError> {
    recent("user_id", user_id, limit)
}

fn recent(column: &str, id: i64, limit: u64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new(column)).eq(id))
        .order_by(Alias::new("at"), Order::Desc)
        .order_by(Alias::new("id"), Order::Desc)
        .limit(limit);
    Statement::query(&query)
}
