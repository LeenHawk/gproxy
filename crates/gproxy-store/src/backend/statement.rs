use sea_query::{OptionEnum, QueryStatementWriter, SqliteQueryBuilder, Value, Values};

use super::DbValue;
use crate::StoreError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Statement {
    pub sql: String,
    pub args: Vec<DbValue>,
}

impl Statement {
    pub(crate) fn plain(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            args: Vec::new(),
        }
    }

    pub(crate) fn query(statement: &impl QueryStatementWriter) -> Result<Self, StoreError> {
        let (sql, values) = statement.build(SqliteQueryBuilder);
        Ok(Self {
            sql,
            args: values_to_db(values)?,
        })
    }
}

fn values_to_db(values: Values) -> Result<Vec<DbValue>, StoreError> {
    values.0.into_iter().map(value_to_db).collect()
}

fn value_to_db(value: Value) -> Result<DbValue, StoreError> {
    let invalid = || StoreError::Database("query contains an unsupported SQLite value".into());
    Ok(match value {
        Value::Bool(value) => option(value.map(i64::from)),
        Value::TinyInt(value) => option(value.map(i64::from)),
        Value::SmallInt(value) => option(value.map(i64::from)),
        Value::Int(value) => option(value.map(i64::from)),
        Value::BigInt(value) => option(value),
        Value::TinyUnsigned(value) => option(value.map(i64::from)),
        Value::SmallUnsigned(value) => option(value.map(i64::from)),
        Value::Unsigned(value) => option(value.map(i64::from)),
        Value::BigUnsigned(value) => option(
            value
                .map(i64::try_from)
                .transpose()
                .map_err(|_| invalid())?,
        ),
        Value::Float(value) => value.map_or(DbValue::Null, |value| DbValue::Real(value.into())),
        Value::Double(value) => value.map_or(DbValue::Null, DbValue::Real),
        Value::String(value) => value.map_or(DbValue::Null, DbValue::Text),
        Value::Char(value) => value.map_or(DbValue::Null, |value| DbValue::Text(value.to_string())),
        Value::Bytes(value) => value.map_or(DbValue::Null, DbValue::Blob),
        Value::Enum(OptionEnum::Some(value)) => DbValue::Text(value.value.to_string()),
        Value::Enum(OptionEnum::None(_)) => DbValue::Null,
    })
}

fn option(value: Option<i64>) -> DbValue {
    value.map_or(DbValue::Null, DbValue::Integer)
}
