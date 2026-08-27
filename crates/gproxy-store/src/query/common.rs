use rust_decimal::Decimal;
use sea_query::{Alias, Asterisk, Expr, ExprTrait, Query, SimpleExpr};
use serde_json::Value;

use crate::StoreError;
use crate::backend::Statement;

pub(super) fn insert(
    table: &'static str,
    columns: &[&'static str],
    values: Vec<SimpleExpr>,
) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new(table))
        .columns(columns.iter().copied().map(Alias::new))
        .values_panic(values)
        .returning_col(Alias::new("id"));
    Statement::query(&query)
}

pub(super) fn select_all(
    table: &'static str,
    columns: &[&'static str],
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(columns.iter().copied().map(Alias::new))
        .from(Alias::new(table));
    if columns.contains(&"id") {
        query.order_by(Alias::new("id"), sea_query::Order::Asc);
    }
    Statement::query(&query)
}

pub(crate) fn count_all(table: &'static str) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .expr_as(Expr::col(Asterisk).count(), Alias::new("count"))
        .from(Alias::new(table));
    Statement::query(&query)
}

pub(super) fn update(
    table: &'static str,
    id: i64,
    columns: &[&'static str],
    values: Vec<SimpleExpr>,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query.table(Alias::new(table));
    for (column, value) in columns.iter().zip(values) {
        query.value(Alias::new(*column), value);
    }
    query.and_where(Expr::col(Alias::new("id")).eq(id));
    Statement::query(&query)
}

pub(super) fn value<T>(value: T) -> SimpleExpr
where
    T: Into<sea_query::Value>,
{
    Expr::value(value)
}

pub(super) fn json(value: &Value, field: &'static str) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|error| StoreError::InvalidData {
        field,
        message: error.to_string(),
    })
}

pub(super) fn decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

pub(super) fn unsigned(value: u64, field: &'static str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::InvalidData {
        field,
        message: "value exceeds SQLite integer range".into(),
    })
}

pub(super) fn unsigned32(value: u32) -> i64 {
    i64::from(value)
}
