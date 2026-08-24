use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, select_all, value};
use crate::records::{CaptureInput, RequestLogInput};

pub(crate) fn add_quota_window(
    quota_id: i64,
    window_start: i64,
    delta: i64,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("quota_windows"))
        .columns([
            Alias::new("quota_id"),
            Alias::new("window_start"),
            Alias::new("used_tokens"),
        ])
        .values_panic([value(quota_id), value(window_start), value(delta)])
        .on_conflict(
            OnConflict::columns([Alias::new("quota_id"), Alias::new("window_start")])
                .value(
                    Alias::new("used_tokens"),
                    Expr::col(Alias::new("used_tokens")).add(delta),
                )
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn read_quota_window(quota_id: i64, window_start: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns([
            Alias::new("quota_id"),
            Alias::new("window_start"),
            Alias::new("used_tokens"),
        ])
        .from(Alias::new("quota_windows"))
        .and_where(Expr::col(Alias::new("quota_id")).eq(quota_id))
        .and_where(Expr::col(Alias::new("window_start")).eq(window_start))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn select_quota_windows() -> Result<Statement, StoreError> {
    select_all(
        "quota_windows",
        &["id", "quota_id", "window_start", "used_tokens"],
    )
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
