//! Ordered batches executed in one worker command, with per-SQL cache reuse.

use super::{execute, ConnectionState};
use crate::{SqliteArguments, SqliteConnection, SqliteQueryResult, SqliteRow};
use sqlx_core::{error::Error, sql_str::SqlStr, Either};

/// One independently bound and cached statement in a batch.
#[derive(Debug, Clone)]
pub struct SqliteBatchStatement {
    pub sql: SqlStr,
    pub arguments: SqliteArguments,
    /// Collect query/RETURNING rows; execution metadata is always retained.
    pub collect_rows: bool,
}

#[derive(Debug, Default)]
pub struct SqliteBatchResult {
    pub result: SqliteQueryResult,
    pub rows: Vec<SqliteRow>,
}

/// The first failed statement. Earlier statements are not automatically rolled back.
#[derive(Debug)]
pub struct SqliteBatchError {
    pub index: usize,
    pub error: Error,
}

impl std::fmt::Display for SqliteBatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "batch statement {}: {}", self.index, self.error)
    }
}
impl std::error::Error for SqliteBatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl SqliteConnection {
    /// Execute an ordered, fully buffered batch in one worker command.
    ///
    /// Each SQL template uses the connection's existing bounded statement cache.
    /// Arguments are scoped to their own statement, never a concatenated script.
    /// Stops at the first error. Use an explicit transaction for atomicity.
    pub async fn execute_batch(
        &mut self,
        statements: Vec<SqliteBatchStatement>,
    ) -> Result<Vec<SqliteBatchResult>, SqliteBatchError> {
        self.worker.execute_batch(statements).await
    }
}

pub(super) fn run(
    conn: &mut ConnectionState,
    statements: Vec<SqliteBatchStatement>,
) -> Result<Vec<SqliteBatchResult>, SqliteBatchError> {
    let mut results = Vec::with_capacity(statements.len());
    for (index, statement) in statements.into_iter().enumerate() {
        let fail = |error| SqliteBatchError { index, error };
        let iter =
            execute::iter(conn, statement.sql, Some(statement.arguments), true).map_err(fail)?;
        let mut output = SqliteBatchResult::default();
        for result in iter {
            match result.map_err(fail)? {
                Either::Left(done) => output.result.extend([done]),
                Either::Right(row) if statement.collect_rows => output.rows.push(row),
                Either::Right(_) => {}
            }
        }
        results.push(output);
    }
    Ok(results)
}
