use sea_query::{Alias, Expr, ExprTrait, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::records::{UsageAggregateQuery, UsageGroupBy};

pub(crate) fn aggregate(
    input: &UsageAggregateQuery,
    after_id: i64,
    limit: u64,
) -> Result<Statement, StoreError> {
    let group = match input.group_by {
        UsageGroupBy::UserKey => "user_key_id",
        UsageGroupBy::User => "user_id",
        UsageGroupBy::Provider => "provider_id",
        UsageGroupBy::Model => "upstream_model",
    };
    let group_expr = if input.group_by == UsageGroupBy::Model {
        Expr::col(Alias::new(group))
    } else {
        Expr::col(Alias::new(group)).cast_as("TEXT")
    };
    let mut query = Query::select();
    query
        .expr_as(group_expr, Alias::new("group_key"))
        .column(Alias::new("id"))
        .columns(
            [
                "input_tokens",
                "output_tokens",
                "cached_input_tokens",
                "cost",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new("at")).gte(input.from))
        .and_where(Expr::col(Alias::new("at")).lt(input.to))
        .and_where(Expr::col(Alias::new("id")).gt(after_id))
        .and_where(Expr::col(Alias::new(group)).is_not_null())
        .order_by(Alias::new("id"), Order::Asc)
        .limit(limit);
    for (column, value) in [
        ("user_key_id", input.user_key_id),
        ("user_id", input.user_id),
        ("provider_id", input.provider_id),
    ] {
        if let Some(value) = value {
            query.and_where(Expr::col(Alias::new(column)).eq(value));
        }
    }
    if let Some(model) = &input.model {
        query.and_where(Expr::col(Alias::new("upstream_model")).eq(model));
    }
    Statement::query(&query)
}
