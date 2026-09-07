use crate::records::{
    CredentialQuotaCycleRecord, CredentialQuotaObservation, CycleObservationRecord,
};
use crate::{StoreError, backend::Statement};
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Order, Query};

pub(crate) fn insert_cycle_observation(
    cycle: &CredentialQuotaCycleRecord,
    raw: &CredentialQuotaObservation,
    rejected: bool,
) -> Result<Statement, StoreError> {
    let mut sample = CycleObservationRecord::from(cycle);
    sample.raw = Some(raw.clone());
    sample.rejected = rejected;
    sample.started_at_ms = raw.sample.started_at_ms;
    sample.observed_at_ms = raw.sample.received_at_ms;
    if rejected {
        sample.uncertain = true;
        sample.unit = raw.unit.clone();
        sample.scope = raw.scope.clone();
        sample.upstream_used = raw.upstream_used;
        sample.upstream_limit = raw.upstream_limit;
        sample.used_percent = raw.used_percent;
    }
    let snapshot =
        serde_json::to_string(&sample).map_err(|error| StoreError::Database(error.to_string()))?;
    let tracking = serde_json::to_string(&cycle.tracking)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    let mut selected = Query::select();
    selected
        .column(Alias::new("id"))
        .expr(Expr::val(sample.started_at_ms))
        .expr(Expr::val(sample.observed_at_ms))
        .expr(Expr::val(snapshot))
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("credential_id")).eq(cycle.credential_id))
        .and_where(Expr::col(Alias::new("window_key")).eq(&cycle.window_key))
        .and_where(Expr::col(Alias::new("version")).eq(cycle.version))
        .and_where(Expr::col(Alias::new("tracking_json")).eq(tracking));
    if cycle.id == 0 {
        selected.and_where(Expr::col(Alias::new("status")).eq("open"));
    } else {
        selected.and_where(Expr::col(Alias::new("id")).eq(cycle.id));
    }
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_observations"))
        .columns(
            [
                "cycle_id",
                "started_at_ms",
                "observed_at_ms",
                "snapshot_json",
            ]
            .map(Alias::new),
        )
        .select_from(selected)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .on_conflict(
            OnConflict::columns(["cycle_id", "observed_at_ms", "started_at_ms"].map(Alias::new))
                .do_nothing()
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn cycle_observations(
    cycle: &CredentialQuotaCycleRecord,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("snapshot_json"))
        .from(Alias::new("credential_quota_observations"))
        .and_where(Expr::col(Alias::new("cycle_id")).eq(cycle.id))
        .and_where(
            Expr::col(Alias::new("observed_at_ms")).lte(cycle.tracking.sample.received_at_ms),
        )
        .order_by(Alias::new("observed_at_ms"), Order::Asc)
        .order_by(Alias::new("started_at_ms"), Order::Asc);
    Statement::query(&query)
}
