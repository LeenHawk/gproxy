use rust_decimal::Decimal;
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, select_all, value};

pub(crate) fn insert_quota_window(
    quota_id: i64,
    window_kind: &str,
    window_start: i64,
    reset_at: Option<i64>,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("quota_windows"))
        .columns([
            Alias::new("quota_id"),
            Alias::new("window_kind"),
            Alias::new("window_start"),
            Alias::new("reset_at"),
            Alias::new("cost_used"),
            Alias::new("active_slot"),
        ])
        .values_panic([
            value(quota_id),
            value(window_kind.to_owned()),
            value(window_start),
            value(reset_at),
            value(decimal(Decimal::ZERO)),
            value(1),
        ])
        .on_conflict(OnConflict::new().do_nothing().to_owned());
    Statement::query(&query)
}

pub(crate) fn update_quota_window_cost(
    id: i64,
    expected_cost: Decimal,
    cost_used: Decimal,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("quota_windows"))
        .value(Alias::new("cost_used"), value(decimal(cost_used)))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("cost_used")).eq(decimal(expected_cost)));
    Statement::query(&query)
}

pub(crate) fn close_quota_window(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("quota_windows"))
        .value(Alias::new("active_slot"), value(Option::<i64>::None))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("active_slot")).eq(1));
    Statement::query(&query)
}

pub(crate) fn read_quota_window(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(quota_window_columns())
        .from(Alias::new("quota_windows"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn read_active_quota_window(
    quota_id: i64,
    window_kind: &str,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(quota_window_columns())
        .from(Alias::new("quota_windows"))
        .and_where(Expr::col(Alias::new("quota_id")).eq(quota_id))
        .and_where(Expr::col(Alias::new("window_kind")).eq(window_kind))
        .and_where(Expr::col(Alias::new("active_slot")).eq(1))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn select_quota_windows() -> Result<Statement, StoreError> {
    select_all(
        "quota_windows",
        &[
            "id",
            "quota_id",
            "window_kind",
            "window_start",
            "reset_at",
            "cost_used",
        ],
    )
}

fn quota_window_columns() -> [Alias; 6] {
    [
        Alias::new("id"),
        Alias::new("quota_id"),
        Alias::new("window_kind"),
        Alias::new("window_start"),
        Alias::new("reset_at"),
        Alias::new("cost_used"),
    ]
}
