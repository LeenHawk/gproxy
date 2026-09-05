use sea_query::{Alias, Expr, ExprTrait, Order, Query, SelectStatement};

use crate::StoreError;
use crate::backend::Statement;
use crate::records::UsageFilter;

fn filtered(filter: &UsageFilter) -> SelectStatement {
    let mut query = Query::select();
    query
        .from(Alias::new("usage_rows"))
        .and_where(Expr::col(Alias::new("at")).gte(filter.from))
        .and_where(Expr::col(Alias::new("at")).lt(filter.to));
    for (column, id) in [
        ("user_key_id", filter.user_key_id),
        ("user_id", filter.user_id),
        ("provider_id", filter.provider_id),
        ("credential_id", filter.credential_id),
    ] {
        if let Some(id) = id {
            query.and_where(Expr::col(Alias::new(column)).eq(id));
        }
    }
    for (column, text) in [
        ("upstream_model", filter.model.as_deref()),
        ("request_id", filter.request_id.as_deref()),
        ("operation", filter.operation.as_deref()),
        ("usage_source", filter.usage_source.as_deref()),
        ("ended", filter.ended.as_deref()),
    ] {
        if let Some(text) = text {
            query.and_where(Expr::col(Alias::new(column)).eq(text));
        }
    }
    query
}

pub(crate) fn records(
    filter: &UsageFilter,
    offset: u64,
    limit: u64,
) -> Result<Statement, StoreError> {
    let mut query = filtered(filter);
    query
        .column(Alias::new("id"))
        .columns(super::row::COLUMNS.iter().copied().map(Alias::new))
        .order_by(Alias::new("at"), Order::Desc)
        .order_by(Alias::new("id"), Order::Desc)
        .offset(offset)
        .limit(limit);
    Statement::query(&query)
}

pub(crate) fn count_filtered(filter: &UsageFilter) -> Result<Statement, StoreError> {
    let mut query = filtered(filter);
    query.expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"));
    Statement::query(&query)
}

pub(crate) fn summary_rows(
    filter: &UsageFilter,
    after: i64,
    limit: u64,
) -> Result<Statement, StoreError> {
    let mut query = filtered(filter);
    query
        .column(Alias::new("id"))
        .columns(super::row::COLUMNS.iter().copied().map(Alias::new))
        .and_where(Expr::col(Alias::new("id")).gt(after))
        .order_by(Alias::new("id"), Order::Asc)
        .limit(limit);
    Statement::query(&query)
}

pub(crate) fn active_credentials(since: i64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("credential_id"))
        .distinct()
        .from(Alias::new("credential_quota_activity"))
        .and_where(Expr::col(Alias::new("started_at_ms")).gte(since * 1000));
    Statement::query(&query)
}
