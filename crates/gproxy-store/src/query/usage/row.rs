use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, json, unsigned, value};
use crate::records::UsageInput;

const COLUMNS: &[&str] = &[
    "request_id",
    "at",
    "provider_id",
    "credential_id",
    "organization_id",
    "team_id",
    "user_id",
    "user_key_id",
    "operation",
    "upstream_model",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "metrics_json",
    "dimensions_json",
    "cost",
    "usage_source",
    "ended",
    "latency_ms",
];

pub(crate) fn insert_usage(input: &UsageInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("usage_rows"))
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .values_panic([
            value(input.request_id.clone()),
            value(input.at),
            value(input.provider_id),
            value(input.credential_id),
            value(input.organization_id),
            value(input.team_id),
            value(input.user_id),
            value(input.user_key_id),
            value(input.operation.clone()),
            value(input.upstream_model.clone()),
            value(unsigned(input.input_tokens, "input_tokens")?),
            value(unsigned(input.output_tokens, "output_tokens")?),
            value(unsigned(input.cached_input_tokens, "cached_input_tokens")?),
            value(json(&input.metrics, "metrics")?),
            value(json(&input.dimensions, "dimensions")?),
            value(decimal(input.cost)),
            value(input.usage_source.clone()),
            value(input.ended.clone()),
            value(unsigned(input.latency_ms, "latency_ms")?),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("request_id"))
                .do_nothing()
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn usage_by_request(request_id: &str) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("id"))
        .columns(COLUMNS.iter().copied().map(Alias::new))
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new("request_id")).eq(request_id))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn usage_count() -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
        .from(Alias::new("usage_rows"));
    Statement::query(&query)
}
