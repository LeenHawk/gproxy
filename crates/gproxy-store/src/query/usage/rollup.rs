use sea_query::{Alias, Expr, ExprTrait, Func, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, json, unsigned, value};
use crate::records::UsageInput;

pub(crate) fn accumulate_hourly(input: &UsageInput) -> Result<Statement, StoreError> {
    let bucket = input.at - input.at.rem_euclid(3_600);
    let dimension_key = json(
        &serde_json::json!([
            input.provider_id,
            input.organization_id,
            input.team_id,
            input.user_id,
            input.upstream_model,
            input.dimensions,
        ]),
        "rollup dimensions",
    )?;
    let mut conflict = OnConflict::columns([
        Alias::new("granularity"),
        Alias::new("bucket_start"),
        Alias::new("dimension_key"),
    ]);
    for column in [
        "requests",
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
    ] {
        conflict.value(
            Alias::new(column),
            Expr::col((Alias::new("usage_rollups"), Alias::new(column)))
                .add(Expr::col((Alias::new("excluded"), Alias::new(column)))),
        );
    }
    conflict
        .value(
            Alias::new("cost"),
            Expr::col((Alias::new("usage_rollups"), Alias::new("cost")))
                .cast_as("NUMERIC")
                .add(Expr::col((Alias::new("excluded"), Alias::new("cost"))).cast_as("NUMERIC"))
                .cast_as("TEXT"),
        )
        .update_column(Alias::new("metrics_json"))
        .value(
            Alias::new("version"),
            Expr::col((Alias::new("usage_rollups"), Alias::new("version"))).add(1),
        );
    let mut query = Query::insert();
    query.into_table(Alias::new("usage_rollups")).columns(
        [
            "granularity",
            "bucket_start",
            "dimension_key",
            "provider_id",
            "organization_id",
            "team_id",
            "user_id",
            "upstream_model",
            "requests",
            "input_tokens",
            "output_tokens",
            "cached_input_tokens",
            "metrics_json",
            "cost",
            "version",
        ]
        .into_iter()
        .map(Alias::new),
    );
    let mut values = Query::select();
    values
        .exprs([
            value("hour"),
            value(bucket),
            value(dimension_key),
            value(input.provider_id),
            value(input.organization_id),
            value(input.team_id),
            value(input.user_id),
            value(input.upstream_model.clone()),
            value(1_i64),
            value(unsigned(input.input_tokens, "input_tokens")?),
            value(unsigned(input.output_tokens, "output_tokens")?),
            value(unsigned(input.cached_input_tokens, "cached_input_tokens")?),
            value(json(&input.metrics, "metrics")?),
            value(decimal(input.cost)),
            value(1_i64),
        ])
        .and_where(Expr::from(Func::cust(Alias::new("changes"))).eq(1));
    query
        .select_from(values)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .on_conflict(conflict.to_owned());
    Statement::query(&query)
}

pub(crate) fn aggregate_for_caller(
    user_id: i64,
    provider_id: i64,
    since: i64,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .expr_as(
            Expr::cust("CAST(COALESCE(SUM(CAST(cost AS NUMERIC)), 0) AS TEXT)"),
            Alias::new("cost"),
        )
        .expr_as(
            Expr::cust("CAST(COALESCE(SUM(input_tokens), 0) AS BIGINT)"),
            Alias::new("input_tokens"),
        )
        .expr_as(
            Expr::cust("CAST(COALESCE(SUM(output_tokens), 0) AS BIGINT)"),
            Alias::new("output_tokens"),
        )
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new("user_id")).eq(user_id))
        .and_where(Expr::col(Alias::new("provider_id")).eq(provider_id))
        .and_where(Expr::col(Alias::new("at")).gte(since));
    Statement::query(&query)
}
