use sea_query::{Alias, Expr, ExprTrait, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::records::{UsageAggregateQuery, UsageGroupBy};

pub(crate) fn aggregate(
    input: &UsageAggregateQuery,
    after_id: i64,
    limit: u64,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("id"))
        .columns(
            [
                "user_key_id",
                "user_id",
                "provider_id",
                "upstream_model",
                "input_tokens",
                "output_tokens",
                "cached_input_tokens",
                "metrics_json",
                "cost",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new("at")).gte(input.from))
        .and_where(Expr::col(Alias::new("at")).lt(input.to))
        .and_where(Expr::col(Alias::new("id")).gt(after_id))
        .order_by(Alias::new("id"), Order::Asc)
        .limit(limit);
    let group = match input.group_by {
        UsageGroupBy::UserKey => Some("user_key_id"),
        UsageGroupBy::User => Some("user_id"),
        UsageGroupBy::Provider => Some("provider_id"),
        UsageGroupBy::Model => Some("upstream_model"),
        UsageGroupBy::Dimensions => None,
    };
    if let Some(group) = group {
        query.and_where(Expr::col(Alias::new(group)).is_not_null());
    }
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
