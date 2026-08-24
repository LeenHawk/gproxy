use std::path::PathBuf;
use std::time::Duration;

use tokio_rusqlite::rusqlite;
use tokio_rusqlite::rusqlite::params_from_iter;
use tokio_rusqlite::rusqlite::types::Value;

use super::{DbValue, Executor, QueryResult, Row, Statement};
use crate::StoreError;

pub(super) struct NativeSql {
    connection: tokio_rusqlite::Connection,
}

impl NativeSql {
    pub(super) async fn open(path: PathBuf) -> Result<Self, StoreError> {
        let connection = tokio_rusqlite::Connection::open(path)
            .await
            .map_err(database_error)?;
        connection
            .call(|connection| -> rusqlite::Result<()> {
                connection.busy_timeout(Duration::from_secs(5))?;
                connection.pragma_update(None, "foreign_keys", true)?;
                connection.pragma_update(None, "journal_mode", "WAL")?;
                connection.pragma_update(None, "synchronous", "NORMAL")?;
                connection.pragma_update(None, "temp_store", "MEMORY")?;
                Ok(())
            })
            .await
            .map_err(database_error)?;
        Ok(Self { connection })
    }
}

impl Executor for NativeSql {
    fn execute<'a>(&'a self, statement: Statement) -> super::DbFuture<'a, QueryResult> {
        let connection = self.connection.clone();
        Box::pin(async move {
            connection
                .call(move |connection| run(connection, statement))
                .await
                .map_err(database_error)
        })
    }

    fn batch<'a>(&'a self, statements: Vec<Statement>) -> super::DbFuture<'a, Vec<QueryResult>> {
        let connection = self.connection.clone();
        Box::pin(async move {
            connection
                .call(move |connection| -> rusqlite::Result<Vec<QueryResult>> {
                    let transaction = connection.transaction()?;
                    let results = statements
                        .into_iter()
                        .map(|statement| run(&transaction, statement))
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    transaction.commit()?;
                    Ok(results)
                })
                .await
                .map_err(database_error)
        })
    }
}

fn run(connection: &rusqlite::Connection, statement: Statement) -> rusqlite::Result<QueryResult> {
    let Statement { sql, args } = statement;
    let mut prepared = connection.prepare(&sql)?;
    let readonly = prepared.readonly();
    let column_names = (0..prepared.column_count())
        .map(|index| prepared.column_name(index).map(str::to_owned))
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let values = args.into_iter().map(to_sql_value);

    if column_names.is_empty() {
        let affected_rows = prepared.execute(params_from_iter(values))? as u64;
        return Ok(QueryResult {
            rows: Vec::new(),
            affected_rows,
            last_insert_id: write_row_id(connection, readonly, affected_rows),
        });
    }

    let mut rows = Vec::new();
    let mut query = prepared.query(params_from_iter(values))?;
    while let Some(row) = query.next()? {
        let values = column_names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let value = row.get::<_, Value>(index)?;
                Ok((name.clone(), from_sql_value(value)))
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.push(Row::new(values));
    }
    drop(query);
    let affected_rows = if readonly { 0 } else { connection.changes() };
    Ok(QueryResult {
        rows,
        affected_rows,
        last_insert_id: write_row_id(connection, readonly, affected_rows),
    })
}

fn write_row_id(
    connection: &rusqlite::Connection,
    readonly: bool,
    affected_rows: u64,
) -> Option<i64> {
    (!readonly && affected_rows > 0).then(|| connection.last_insert_rowid())
}

fn to_sql_value(value: DbValue) -> Value {
    match value {
        DbValue::Null => Value::Null,
        DbValue::Integer(value) => Value::Integer(value),
        DbValue::Real(value) => Value::Real(value),
        DbValue::Text(value) => Value::Text(value),
        DbValue::Blob(value) => Value::Blob(value),
    }
}

fn from_sql_value(value: Value) -> DbValue {
    match value {
        Value::Null => DbValue::Null,
        Value::Integer(value) => DbValue::Integer(value),
        Value::Real(value) => DbValue::Real(value),
        Value::Text(value) => DbValue::Text(value),
        Value::Blob(value) => DbValue::Blob(value),
    }
}

fn database_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Database(error.to_string())
}
