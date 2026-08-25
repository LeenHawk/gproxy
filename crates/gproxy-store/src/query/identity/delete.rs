use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;

pub(crate) fn delete_permission(id: i64) -> Result<Statement, StoreError> {
    delete_by_id("permissions", id)
}

pub(crate) fn delete_rate_limit(id: i64) -> Result<Statement, StoreError> {
    delete_by_id("rate_limits", id)
}

pub(crate) fn delete_quota(id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("quotas"))
        .value(Alias::new("enabled"), false)
        .and_where(Expr::col(Alias::new("id")).eq(id));
    Statement::query(&query)
}

fn delete_by_id(table: &'static str, id: i64) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new(table))
        .and_where(Expr::col(Alias::new("id")).eq(id));
    Statement::query(&query)
}
