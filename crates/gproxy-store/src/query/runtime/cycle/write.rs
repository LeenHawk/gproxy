use crate::query::common::{decimal, json, value};
use crate::records::CredentialQuotaCycleRecord;
use crate::{StoreError, backend::Statement};
use sea_query::{Alias, Expr, ExprTrait, Query, SimpleExpr};

fn values(record: &CredentialQuotaCycleRecord) -> Result<Vec<SimpleExpr>, StoreError> {
    Ok(vec![
        value(record.version),
        value(record.credential_id),
        value(record.window_key.clone()),
        value(record.period_start),
        value(record.period_end),
        value(text(&record.boundary_source)?),
        value(text(&record.boundary_confidence)?),
        value(text(&record.status)?),
        value(
            record
                .close_reason
                .map(|reason| text(&reason))
                .transpose()?,
        ),
        value((record.status == crate::records::QuotaCycleStatus::Open).then_some(1)),
        value(record.last_observed_at),
        value(record.upstream_used.map(decimal)),
        value(record.upstream_limit.map(decimal)),
        value(record.used_percent.map(decimal)),
        value(text(&record.coverage)?),
        value(json(&record.metrics, "metrics")?),
        value(record.label.clone()),
        value(record.accounting_start_ms),
        value(record.accounting_end_ms),
        value(
            serde_json::to_string(&record.tracking)
                .map_err(|error| StoreError::Database(error.to_string()))?,
        ),
    ])
}

pub(crate) fn insert_tracked_cycle(
    record: &CredentialQuotaCycleRecord,
    previous: Option<&CredentialQuotaCycleRecord>,
) -> Result<Statement, StoreError> {
    let mut selected = Query::select();
    selected.exprs(values(record)?);
    if let Some(previous) = previous {
        let mut guard = Query::select();
        guard
            .expr(Expr::val(1))
            .from(Alias::new("credential_quota_cycles"))
            .and_where(Expr::col(Alias::new("id")).eq(previous.id))
            .and_where(Expr::col(Alias::new("version")).eq(previous.version))
            .and_where(Expr::col(Alias::new("status")).eq("closed"));
        selected.and_where(Expr::exists(guard));
        let mut newer = Query::select();
        newer
            .expr(Expr::val(1))
            .from(Alias::new("credential_quota_cycles"))
            .and_where(Expr::col(Alias::new("credential_id")).eq(record.credential_id))
            .and_where(Expr::col(Alias::new("window_key")).eq(&record.window_key))
            .and_where(Expr::col(Alias::new("id")).gt(previous.id));
        selected.and_where(Expr::exists(newer).not());
    }
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_cycles"))
        .columns(super::COLUMNS[1..].iter().copied().map(Alias::new))
        .select_from(selected)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

pub(crate) fn update_tracked_cycle(
    record: &CredentialQuotaCycleRecord,
    expected: u64,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values(
            super::COLUMNS[1..]
                .iter()
                .copied()
                .map(Alias::new)
                .zip(values(record)?),
        )
        .and_where(Expr::col(Alias::new("id")).eq(record.id))
        .and_where(Expr::col(Alias::new("version")).eq(expected));
    Statement::query(&query)
}

pub(crate) fn update_cycle_after_link(
    record: &CredentialQuotaCycleRecord,
    expected: u64,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credential_quota_cycles"))
        .values(
            super::COLUMNS[1..]
                .iter()
                .copied()
                .map(Alias::new)
                .zip(values(record)?),
        )
        .and_where(Expr::col(Alias::new("id")).eq(record.id))
        .and_where(Expr::col(Alias::new("version")).eq(expected))
        .and_where(Expr::from(sea_query::Func::cust(Alias::new("changes"))).gt(0));
    Statement::query(&query)
}

fn text(value: &impl serde::Serialize) -> Result<String, StoreError> {
    serde_json::to_value(value)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| StoreError::Database("invalid enum".into()))
}
