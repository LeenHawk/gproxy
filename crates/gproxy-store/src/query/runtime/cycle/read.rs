use sea_query::{Alias, Expr, ExprTrait, Order, OverStatement, Query, WindowStatement};

use super::COLUMNS;
use crate::StoreError;
use crate::backend::Statement;

pub(crate) fn read_credential_quota_cycle(id: i64) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query.and_where(Expr::col(Alias::new("id")).eq(id)).limit(1);
    Statement::query(&query)
}

pub(crate) fn read_open_credential_quota_cycle(
    credential_id: i64,
    window_key: &str,
) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
        .and_where(Expr::col(Alias::new("window_key")).eq(window_key))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn read_latest_credential_quota_cycle(
    credential_id: i64,
    window_key: &str,
) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
        .and_where(Expr::col(Alias::new("window_key")).eq(window_key))
        .order_by(Alias::new("id"), Order::Desc)
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn select_open_credential_quota_cycles(
    credential_id: Option<i64>,
) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query.and_where(Expr::col(Alias::new("open_slot")).eq(1));
    if let Some(credential_id) = credential_id {
        query.and_where(Expr::col(Alias::new("credential_id")).eq(credential_id));
    }
    query.order_by(Alias::new("id"), Order::Asc);
    Statement::query(&query)
}

pub(crate) fn select_credential_quota_cycle_history(
    credential_id: i64,
    window_key: &str,
) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
        .and_where(Expr::col(Alias::new("window_key")).eq(window_key))
        .order_by(Alias::new("id"), Order::Desc);
    Statement::query(&query)
}

/// A busy window must not push quieter ones out of the page, so the row budget
/// is spent per `(credential, window)` rather than across the whole range.
const CYCLES_PER_WINDOW: u64 = 100;
const CYCLE_ROWS: u64 = 5_000;

pub(crate) fn select_credential_quota_cycles(
    credential_id: Option<i64>,
    from: i64,
    to: i64,
) -> Result<Statement, StoreError> {
    let mut ranked = cycle_select();
    if let Some(credential_id) = credential_id {
        ranked.and_where(Expr::col(Alias::new("credential_id")).eq(credential_id));
    }
    let mut window = WindowStatement::partition_by(Alias::new("credential_id"));
    OverStatement::partition_by(&mut window, Alias::new("window_key"));
    window
        .order_by(Alias::new("last_observed_at"), Order::Desc)
        .order_by(Alias::new("id"), Order::Desc);
    ranked
        .and_where(Expr::col(Alias::new("last_observed_at")).gte(from))
        .and_where(Expr::col(Alias::new("last_observed_at")).lt(to))
        .expr_window_as(
            Expr::cust("ROW_NUMBER()"),
            window,
            Alias::new("window_rank"),
        );

    let mut query = Query::select();
    query
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from_subquery(ranked, Alias::new("ranked"))
        .and_where(Expr::col(Alias::new("window_rank")).lte(CYCLES_PER_WINDOW))
        .order_by(Alias::new("last_observed_at"), Order::Desc)
        .order_by(Alias::new("id"), Order::Desc)
        .limit(CYCLE_ROWS);
    Statement::query(&query)
}

fn cycle_select() -> sea_query::SelectStatement {
    let mut query = Query::select();
    query
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("credential_quota_cycles"));
    query.to_owned()
}
