use rust_decimal::Decimal;
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, insert, select_all, value};
use crate::records::{CaptureInput, RequestLogInput};

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

pub(crate) fn begin_request_log(input: &RequestLogInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("request_logs"))
        .columns([
            Alias::new("request_id"),
            Alias::new("at"),
            Alias::new("method"),
            Alias::new("path"),
            Alias::new("query"),
        ])
        .values_panic([
            value(input.request_id.clone()),
            value(input.at),
            value(input.method.clone()),
            value(input.path.clone()),
            value(input.query.clone()),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("request_id"))
                .do_nothing()
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn finish_request_log(
    request_id: &str,
    response_status: Option<u16>,
    error_kind: Option<&str>,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("request_logs"))
        .values([
            (
                Alias::new("response_status"),
                value(response_status.map(i64::from)),
            ),
            (
                Alias::new("error_kind"),
                value(error_kind.map(str::to_owned)),
            ),
        ])
        .and_where(Expr::col(Alias::new("request_id")).eq(request_id));
    Statement::query(&query)
}

pub(crate) fn insert_capture(input: &CaptureInput) -> Result<Statement, StoreError> {
    insert(
        "wire_logs",
        &[
            "request_id",
            "at",
            "provider_id",
            "credential_id",
            "upstream_url",
            "response_status",
            "request_body",
            "response_body",
        ],
        vec![
            value(input.request_id.clone()),
            value(input.at),
            value(input.provider_id),
            value(input.credential_id),
            value(input.upstream_url.clone()),
            value(input.response_status.map(i64::from)),
            value(input.request_body.clone()),
            value(input.response_body.clone()),
        ],
    )
}
