use crate::records::CredentialQuotaCycleRecord;
use crate::{StoreError, backend::Statement};
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

pub(crate) fn begin_usage(
    request: &str,
    credential: i64,
    model: &str,
    at_ms: i64,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_activity"))
        .columns(["request_id", "credential_id", "model", "started_at_ms"].map(Alias::new))
        .values_panic([
            request.into(),
            credential.into(),
            model.into(),
            at_ms.into(),
        ])
        .on_conflict(
            OnConflict::columns(["request_id", "credential_id", "started_at_ms"].map(Alias::new))
                .do_nothing()
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn incomplete_cycle_usage(
    cycle: &CredentialQuotaCycleRecord,
) -> Result<Statement, StoreError> {
    let tracking = &cycle.tracking;
    let mut settled = Query::select();
    settled
        .expr(Expr::val(1))
        .from(Alias::new("usage_rows"))
        .and_where(
            Expr::col((Alias::new("usage_rows"), Alias::new("request_id"))).equals((
                Alias::new("credential_quota_activity"),
                Alias::new("request_id"),
            )),
        )
        .and_where(
            Expr::col((Alias::new("usage_rows"), Alias::new("credential_id"))).equals((
                Alias::new("credential_quota_activity"),
                Alias::new("credential_id"),
            )),
        )
        .and_where(Expr::col(Alias::new("upstream_started_at_ms")).equals((
            Alias::new("credential_quota_activity"),
            Alias::new("started_at_ms"),
        )));
    let mut query = Query::select();
    query
        .column(Alias::new("request_id"))
        .from(Alias::new("credential_quota_activity"))
        .and_where(Expr::col(Alias::new("credential_id")).eq(cycle.credential_id))
        .and_where(Expr::col(Alias::new("started_at_ms")).gte(cycle.accounting_start_ms))
        .and_where(Expr::col(Alias::new("started_at_ms")).lt(tracking.sample.received_at_ms))
        .and_where(Expr::exists(settled).not())
        .limit(1);
    if let gproxy_core::QuotaScope::Models(models) = &tracking.scope {
        query.and_where(Expr::col(Alias::new("model")).is_in(models.iter().cloned()));
    }
    Statement::query(&query)
}
