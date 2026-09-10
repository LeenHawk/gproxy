use gproxy_core::channel_api::QuotaEntry;
use sea_query::{Alias, Expr, ExprTrait, OnConflict, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{unsigned, value};

const TABLE: &str = "credential_quota_response_entries";
const COLUMNS: [&str; 5] = [
    "credential_id",
    "source_id",
    "entry_id",
    "observed_at_ms",
    "entry_json",
];

pub(crate) fn select(credential_id: i64) -> Result<Statement, StoreError> {
    Statement::query(
        Query::select()
            .column(Alias::new("entry_json"))
            .from(Alias::new(TABLE))
            .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
            .order_by(Alias::new("source_id"), Order::Asc)
            .order_by(Alias::new("entry_id"), Order::Asc),
    )
}

pub(crate) fn observe(
    credential_id: i64,
    expected_version: u64,
    entry: &QuotaEntry,
) -> Result<Vec<Statement>, StoreError> {
    let json = serde_json::to_string(entry).map_err(|error| StoreError::InvalidData {
        field: "quota response entry",
        message: error.to_string(),
    })?;
    let mut seed = Query::select();
    seed.exprs([
        value(credential_id),
        value(entry.source_id.clone()),
        value(entry.id.clone()),
        value(entry.observed_at_ms),
        value(json.clone()),
    ])
    .from(Alias::new("credentials"))
    .and_where(Expr::col(Alias::new("id")).eq(credential_id))
    .and_where(
        Expr::col(Alias::new("version")).eq(unsigned(expected_version, "credential version")?),
    );
    let mut insert = Query::insert();
    insert
        .into_table(Alias::new(TABLE))
        .columns(COLUMNS.map(Alias::new))
        .select_from(seed)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .on_conflict(
            OnConflict::columns(COLUMNS[..3].iter().map(|column| Alias::new(*column)))
                .do_nothing()
                .to_owned(),
        );
    let mut update = Query::update();
    update
        .table(Alias::new(TABLE))
        .value(Alias::new("entry_json"), json)
        .value(Alias::new("observed_at_ms"), entry.observed_at_ms)
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
        .and_where(Expr::col(Alias::new("source_id")).eq(&entry.source_id))
        .and_where(Expr::col(Alias::new("entry_id")).eq(&entry.id))
        .and_where(super::quota_snapshot::credential_version_matches(
            credential_id,
            expected_version,
        )?)
        .and_where(Expr::col(Alias::new("observed_at_ms")).lt(entry.observed_at_ms));
    Ok(vec![Statement::query(&insert)?, Statement::query(&update)?])
}
