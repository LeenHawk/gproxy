use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{json, value};
use crate::records::{CredentialQuotaCycleModelRecord, CredentialQuotaObservation};

pub(crate) fn delete_credential_cycle_models(
    cycle_id: i64,
    version: u64,
) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("credential_quota_cycle_models"))
        .and_where(Expr::col(Alias::new("cycle_id")).eq(cycle_id))
        .and_where(Expr::exists(cycle_at_version(cycle_id, version)));
    Statement::query(&query)
}

pub(crate) fn insert_credential_cycle_model(
    cycle_id: i64,
    version: u64,
    model: &CredentialQuotaCycleModelRecord,
) -> Result<Statement, StoreError> {
    let mut values = Query::select();
    values
        .exprs([
            Expr::col(Alias::new("id")),
            value(model.model.clone()),
            value(json(&model.metrics, "model metrics")?),
        ])
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("id")).eq(cycle_id))
        .and_where(Expr::col(Alias::new("version")).eq(version));
    insert_cycle_model(values)
}

pub(crate) fn insert_open_credential_cycle_model(
    input: &CredentialQuotaObservation,
    model: &CredentialQuotaCycleModelRecord,
) -> Result<Statement, StoreError> {
    let mut values = Query::select();
    values
        .exprs([
            Expr::col(Alias::new("id")),
            value(model.model.clone()),
            value(json(&model.metrics, "model metrics")?),
        ])
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("credential_id")).eq(input.credential_id))
        .and_where(Expr::col(Alias::new("window_key")).eq(&input.window_key))
        .and_where(Expr::col(Alias::new("open_slot")).eq(1));
    insert_cycle_model(values)
}

fn insert_cycle_model(values: sea_query::SelectStatement) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("credential_quota_cycle_models"))
        .columns(
            ["cycle_id", "model", "metrics_json"]
                .into_iter()
                .map(Alias::new),
        )
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Statement::query(&query)
}

fn cycle_at_version(cycle_id: i64, version: u64) -> sea_query::SelectStatement {
    let mut query = Query::select();
    query
        .expr(Expr::val(1))
        .from(Alias::new("credential_quota_cycles"))
        .and_where(Expr::col(Alias::new("id")).eq(cycle_id))
        .and_where(Expr::col(Alias::new("version")).eq(version));
    query.to_owned()
}
