use gproxy_core::channel_api::{QuotaEntry, QuotaSourceState};
use sea_query::{Alias, Cond, Expr, ExprTrait, OnConflict, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{unsigned, value};

const TABLE: &str = "credential_quota_sources";
const COLUMNS: [&str; 9] = [
    "credential_id",
    "source_id",
    "capability_json",
    "attempted_at_ms",
    "observed_at_ms",
    "error_code",
    "error_message",
    "entries_json",
    "reset_credits_json",
];

pub(crate) fn select(credential_id: i64) -> Result<Statement, StoreError> {
    Statement::query(
        Query::select()
            .columns(COLUMNS.map(Alias::new))
            .from(Alias::new(TABLE))
            .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
            .order_by(Alias::new("source_id"), Order::Asc),
    )
}

pub(crate) fn save(
    credential_id: i64,
    expected_version: u64,
    state: &QuotaSourceState,
    entries: Option<&[QuotaEntry]>,
) -> Result<Vec<Statement>, StoreError> {
    let attempted = state
        .attempted_at_ms
        .ok_or_else(|| StoreError::InvalidData {
            field: "quota attempt",
            message: "a saved query result must have an attempt time".into(),
        })?;
    let capability = serialize(&state.capability)?;
    let mut seed = Query::select();
    seed.exprs([
        value(credential_id),
        value(state.capability.id.clone()),
        value(capability.clone()),
        value(None::<i64>),
        value(None::<i64>),
        value(None::<String>),
        value(None::<String>),
        value("[]"),
        value(None::<String>),
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
            OnConflict::columns([Alias::new("credential_id"), Alias::new("source_id")])
                .do_nothing()
                .to_owned(),
        );

    let attempt_column = Expr::col(Alias::new("attempted_at_ms"));
    let mut newer = Cond::any()
        .add(attempt_column.clone().is_null())
        .add(attempt_column.clone().lt(attempted));
    // A successful observation wins a same-millisecond tie with an error.
    if entries.is_some() {
        newer = newer.add(
            Cond::all()
                .add(attempt_column.eq(attempted))
                .add(Expr::col(Alias::new("error_code")).is_not_null()),
        );
    }
    let mut update = Query::update();
    update
        .table(Alias::new(TABLE))
        .value(Alias::new("capability_json"), capability)
        .value(Alias::new("attempted_at_ms"), attempted)
        .value(
            Alias::new("error_code"),
            state.error.as_ref().map(|error| error.code.clone()),
        )
        .value(
            Alias::new("error_message"),
            state.error.as_ref().map(|error| error.message.clone()),
        )
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id))
        .and_where(Expr::col(Alias::new("source_id")).eq(&state.capability.id))
        .and_where(credential_version_matches(credential_id, expected_version)?)
        .cond_where(newer);
    if let Some(entries) = entries {
        update
            .value(Alias::new("observed_at_ms"), state.observed_at_ms)
            .value(Alias::new("entries_json"), serialize(entries)?)
            .value(
                Alias::new("reset_credits_json"),
                state.reset_credits.as_ref().map(serialize).transpose()?,
            );
    }
    Ok(vec![Statement::query(&insert)?, Statement::query(&update)?])
}

pub(super) fn credential_version_matches(
    id: i64,
    expected_version: u64,
) -> Result<sea_query::SimpleExpr, StoreError> {
    let mut credential = Query::select();
    credential
        .expr(Expr::val(1))
        .from(Alias::new("credentials"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(
            Expr::col(Alias::new("version")).eq(unsigned(expected_version, "credential version")?),
        );
    Ok(Expr::exists(credential))
}

pub(crate) fn clear(credential_id: i64) -> Result<Vec<Statement>, StoreError> {
    [TABLE, "credential_quota_response_entries"]
        .into_iter()
        .map(|table| {
            Statement::query(
                Query::delete()
                    .from_table(Alias::new(table))
                    .and_where(Expr::col(Alias::new("credential_id")).eq(credential_id)),
            )
        })
        .collect()
}

pub(crate) fn clear_version(id: i64, version: u64) -> Result<Vec<Statement>, StoreError> {
    [TABLE, "credential_quota_response_entries"]
        .into_iter()
        .map(|table| {
            Statement::query(
                Query::delete()
                    .from_table(Alias::new(table))
                    .and_where(Expr::col(Alias::new("credential_id")).eq(id))
                    .and_where(credential_version_matches(id, version)?),
            )
        })
        .collect()
}

pub(crate) fn advance_health_version(id: i64, version: u64) -> Result<Statement, StoreError> {
    Statement::query(
        Query::update()
            .table(Alias::new("credential_health"))
            .value(
                Alias::new("credential_version"),
                unsigned(version + 1, "credential version")?,
            )
            .and_where(Expr::col(Alias::new("credential_id")).eq(id))
            .and_where(
                Expr::col(Alias::new("credential_version"))
                    .eq(unsigned(version, "credential version")?),
            )
            .and_where(credential_version_matches(id, version)?),
    )
}

fn serialize(value: &(impl serde::Serialize + ?Sized)) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|error| StoreError::InvalidData {
        field: "quota snapshot",
        message: error.to_string(),
    })
}
