use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;

pub(super) const DELETE_BATCH_SIZE: u64 = 5_000;

pub(crate) fn delete_before(table: &'static str, cutoff: i64) -> Result<Statement, StoreError> {
    delete_oldest(table, Some(cutoff))
}

pub(crate) fn delete_oldest_logs() -> Result<Vec<Statement>, StoreError> {
    ["request_logs", "wire_logs"]
        .into_iter()
        .map(|table| delete_oldest(table, None))
        .collect()
}

fn delete_oldest(table: &'static str, cutoff: Option<i64>) -> Result<Statement, StoreError> {
    let mut ids = Query::select();
    ids.column(Alias::new("id"))
        .from(Alias::new(table))
        .order_by(Alias::new("at"), sea_query::Order::Asc)
        .order_by(Alias::new("id"), sea_query::Order::Asc)
        .limit(DELETE_BATCH_SIZE);
    if let Some(cutoff) = cutoff {
        ids.and_where(Expr::col(Alias::new("at")).lt(cutoff));
    }
    let mut delete = Query::delete();
    delete
        .from_table(Alias::new(table))
        .and_where(Expr::col(Alias::new("id")).in_subquery(ids));
    Statement::query(&delete)
}

/// Quota activity is live request state, not history; rows older than the
/// retention cutoff belong to requests that can no longer settle.
pub(crate) fn delete_stale_quota_activity(cutoff: i64) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("credential_quota_activity"))
        .and_where(Expr::col(Alias::new("started_at_ms")).lt(cutoff.saturating_mul(1_000)));
    Statement::query(&query)
}
