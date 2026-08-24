use sea_query::{Alias, Condition, Expr, ExprTrait, Order, Query};
use serde::Serialize;

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, json, value};
use crate::records::{CredentialQuotaObservation, QuotaCycleCloseReason, QuotaCycleStatus};

const COLUMNS: &[&str] = &[
    "id",
    "credential_id",
    "window_key",
    "period_start",
    "period_end",
    "boundary_source",
    "boundary_confidence",
    "status",
    "close_reason",
    "open_slot",
    "last_observed_at",
    "upstream_used",
    "upstream_limit",
    "used_percent",
    "coverage",
    "metrics_json",
];

pub(crate) fn insert_credential_quota_cycle(
    input: &CredentialQuotaObservation,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_cycles"))
        .columns(COLUMNS[1..].iter().copied().map(Alias::new))
        .values_panic([
            value(input.credential_id),
            value(input.window_key.clone()),
            value(input.period_start),
            value(input.period_end),
            value(enum_text(&input.boundary_source, "boundary_source")?),
            value(enum_text(
                &input.boundary_confidence,
                "boundary_confidence",
            )?),
            value(enum_text(&QuotaCycleStatus::Open, "status")?),
            value(Option::<String>::None),
            value(1),
            value(input.observed_at),
            value(input.upstream_used.map(decimal)),
            value(input.upstream_limit.map(decimal)),
            value(input.used_percent.map(decimal)),
            value(enum_text(&input.coverage, "coverage")?),
            value(json(&input.metrics, "metrics")?),
        ]);
    Statement::query(&query)
}

pub(crate) fn update_credential_quota_cycle(
    id: i64,
    input: &CredentialQuotaObservation,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values([
            (Alias::new("period_start"), value(input.period_start)),
            (Alias::new("period_end"), value(input.period_end)),
            (
                Alias::new("boundary_source"),
                value(enum_text(&input.boundary_source, "boundary_source")?),
            ),
            (
                Alias::new("boundary_confidence"),
                value(enum_text(
                    &input.boundary_confidence,
                    "boundary_confidence",
                )?),
            ),
            (Alias::new("last_observed_at"), value(input.observed_at)),
            (
                Alias::new("upstream_used"),
                value(input.upstream_used.map(decimal)),
            ),
            (
                Alias::new("upstream_limit"),
                value(input.upstream_limit.map(decimal)),
            ),
            (
                Alias::new("used_percent"),
                value(input.used_percent.map(decimal)),
            ),
            (
                Alias::new("coverage"),
                value(enum_text(&input.coverage, "coverage")?),
            ),
            (
                Alias::new("metrics_json"),
                value(json(&input.metrics, "metrics")?),
            ),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1));
    Statement::query(&query)
}

pub(crate) fn close_credential_quota_cycle(
    id: i64,
    period_end: i64,
    reason: QuotaCycleCloseReason,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values([
            (Alias::new("period_end"), value(period_end)),
            (
                Alias::new("status"),
                value(enum_text(&QuotaCycleStatus::Closed, "status")?),
            ),
            (
                Alias::new("close_reason"),
                value(enum_text(&reason, "close_reason")?),
            ),
            (Alias::new("open_slot"), value(Option::<i64>::None)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1));
    Statement::query(&query)
}

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

pub(crate) fn select_open_credential_quota_cycles(
    credential_id: Option<i64>,
    now: i64,
) -> Result<Statement, StoreError> {
    let mut query = cycle_select();
    query.and_where(Expr::col(Alias::new("open_slot")).eq(1));
    if let Some(credential_id) = credential_id {
        query.and_where(Expr::col(Alias::new("credential_id")).eq(credential_id));
    }
    query
        .cond_where(
            Condition::any()
                .add(Expr::col(Alias::new("period_end")).is_null())
                .add(Expr::col(Alias::new("period_end")).gt(now)),
        )
        .order_by(Alias::new("id"), Order::Asc);
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

fn cycle_select() -> sea_query::SelectStatement {
    let mut query = Query::select();
    query
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("credential_quota_cycles"));
    query.to_owned()
}

fn enum_text(value: &impl Serialize, field: &'static str) -> Result<String, StoreError> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| StoreError::InvalidData {
            field,
            message: "enum did not serialize as text".into(),
        })
}
