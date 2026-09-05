use crate::records::CredentialQuotaCycleRecord;
use crate::{StoreError, backend::Statement};
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Order, Query, SimpleExpr};

fn sent_at() -> SimpleExpr {
    Expr::col((
        Alias::new("usage_rows"),
        Alias::new("upstream_started_at_ms"),
    ))
}

pub(crate) fn cycle_usage_rows(
    cycle: &CredentialQuotaCycleRecord,
    after: i64,
    missing: Option<bool>,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column((Alias::new("usage_rows"), Alias::new("id")))
        .columns(
            crate::query::usage::COLUMNS
                .iter()
                .map(|column| (Alias::new("usage_rows"), Alias::new(*column))),
        )
        .from(Alias::new("usage_rows"))
        .and_where(
            Expr::col((Alias::new("usage_rows"), Alias::new("credential_id")))
                .eq(cycle.credential_id),
        )
        .and_where(Expr::col((Alias::new("usage_rows"), Alias::new("id"))).gt(after))
        .and_where(sent_at().gte(cycle.accounting_start_ms))
        .order_by((Alias::new("usage_rows"), Alias::new("id")), Order::Asc)
        .limit(1000);
    if let Some(end) = cycle.accounting_end_ms {
        query.and_where(sent_at().lt(end));
    }
    if missing == Some(false) {
        let tracking = &cycle.tracking;
        query
            .and_where(sent_at().gte(tracking.baseline_at_ms))
            .and_where(sent_at().lt(tracking.sample.received_at_ms));
    }
    let mut linked = Query::select();
    linked
        .expr(Expr::val(1))
        .from(Alias::new("credential_quota_cycle_usage"))
        .and_where(
            Expr::col(Alias::new("usage_id")).equals((Alias::new("usage_rows"), Alias::new("id"))),
        )
        .and_where(Expr::col(Alias::new("cycle_id")).eq(cycle.id));
    if let Some(missing) = missing {
        query.and_where(if missing {
            Expr::exists(linked).not()
        } else {
            Expr::exists(linked)
        });
    }
    Statement::query(&query)
}

pub(crate) fn link_cycle_usage(
    cycle: &CredentialQuotaCycleRecord,
    usage_id: i64,
) -> Result<Statement, StoreError> {
    let mut selected = Query::select();
    selected
        .expr(Expr::val(usage_id))
        .column(Alias::new("window_key"))
        .column(Alias::new("id"))
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("id")).eq(cycle.id))
        .and_where(Expr::col(Alias::new("version")).eq(cycle.version));
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_cycle_usage"))
        .columns(["usage_id", "window_key", "cycle_id"].map(Alias::new))
        .select_from(selected)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .on_conflict(
            OnConflict::columns([Alias::new("usage_id"), Alias::new("window_key")])
                .update_column(Alias::new("cycle_id"))
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn cycles_for_usage(credential: i64, at_ms: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(super::COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential))
        .and_where(Expr::col(Alias::new("accounting_start_ms")).lte(at_ms))
        .and_where(
            Expr::col(Alias::new("accounting_end_ms"))
                .is_null()
                .or(Expr::col(Alias::new("accounting_end_ms")).gt(at_ms)),
        )
        .order_by(Alias::new("id"), Order::Desc);
    Statement::query(&query)
}

pub(crate) fn read_cycle_usage_link(cycle: i64, usage: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("usage_id"))
        .from(Alias::new("credential_quota_cycle_usage"))
        .and_where(Expr::col(Alias::new("cycle_id")).eq(cycle))
        .and_where(Expr::col(Alias::new("usage_id")).eq(usage));
    Statement::query(&query)
}
