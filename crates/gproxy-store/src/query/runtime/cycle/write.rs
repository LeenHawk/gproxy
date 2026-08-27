use sea_query::{Alias, Expr, ExprTrait, Query};
use serde::Serialize;

use super::COLUMNS;
use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, json, value};
use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, QuotaBoundaryConfidence,
    QuotaBoundarySource, QuotaCoverage, QuotaCycleCloseReason, QuotaCycleStatus,
};

pub(crate) fn insert_credential_quota_cycle(
    input: &CredentialQuotaObservation,
    coverage: QuotaCoverage,
    metrics: &serde_json::Value,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_cycles"))
        .columns(COLUMNS[1..].iter().copied().map(Alias::new))
        .values_panic([
            value(0),
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
            value(enum_text(&coverage, "coverage")?),
            value(json(metrics, "metrics")?),
        ])
        .returning_col(Alias::new("id"));
    Statement::query(&query)
}

pub(crate) fn update_credential_quota_cycle(
    expected: &CredentialQuotaCycleRecord,
    input: &CredentialQuotaObservation,
    coverage: QuotaCoverage,
    metrics: &serde_json::Value,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values([
            (
                Alias::new("version"),
                value(expected.version.saturating_add(1)),
            ),
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
                value(enum_text(&coverage, "coverage")?),
            ),
            (Alias::new("metrics_json"), value(json(metrics, "metrics")?)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(expected.id))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1))
        .and_where(Expr::col(Alias::new("version")).eq(expected.version));
    Statement::query(&query)
}

pub(crate) fn close_credential_quota_cycle(
    expected: &CredentialQuotaCycleRecord,
    period_end: i64,
    boundary_source: QuotaBoundarySource,
    boundary_confidence: QuotaBoundaryConfidence,
    reason: QuotaCycleCloseReason,
    observed_before: Option<i64>,
    metrics: &serde_json::Value,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values([
            (
                Alias::new("version"),
                value(expected.version.saturating_add(1)),
            ),
            (Alias::new("period_end"), value(period_end)),
            (
                Alias::new("boundary_source"),
                value(enum_text(&boundary_source, "boundary_source")?),
            ),
            (
                Alias::new("boundary_confidence"),
                value(enum_text(&boundary_confidence, "boundary_confidence")?),
            ),
            (
                Alias::new("status"),
                value(enum_text(&QuotaCycleStatus::Closed, "status")?),
            ),
            (
                Alias::new("close_reason"),
                value(enum_text(&reason, "close_reason")?),
            ),
            (
                Alias::new("metrics_json"),
                value(json(metrics, "metrics_json")?),
            ),
            (Alias::new("open_slot"), value(Option::<i64>::None)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(expected.id))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1))
        .and_where(Expr::col(Alias::new("version")).eq(expected.version));
    if let Some(observed_at) = observed_before {
        query.and_where(Expr::col(Alias::new("last_observed_at")).lte(observed_at));
    }
    Statement::query(&query)
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
