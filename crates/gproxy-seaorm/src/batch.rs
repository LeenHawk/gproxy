use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, ExecResult, IsolationLevel, QueryResult,
    Statement, TransactionTrait,
};

use crate::{Projection, error};

/// A query's SQL and its named D1 result types. Native drivers decode their own
/// rows. SELECT and backend-supported DML RETURNING are both allowed.
#[derive(Clone, Debug)]
pub struct BatchQuery {
    pub statement: Statement,
    pub projection: Projection,
}

impl BatchQuery {
    pub fn new(statement: Statement, projection: Projection) -> Self {
        Self {
            statement,
            projection,
        }
    }
}

/// One step in an ordered atomic batch. Use Query when returned rows are needed;
/// Execute returns driver execution metadata and discards any returned rows.
#[derive(Clone, Debug)]
pub enum BatchStatement {
    Execute(Statement),
    Query(BatchQuery),
}

impl BatchStatement {
    pub fn statement(&self) -> &Statement {
        match self {
            Self::Execute(statement) => statement,
            Self::Query(query) => &query.statement,
        }
    }

    pub(crate) fn validate(&self, backend: DbBackend) -> Result<(), DbErr> {
        if self.statement().db_backend != backend {
            return Err(error(
                "batch statement dialect does not match the connection",
            ));
        }
        if let Self::Query(query) = self
            && query.projection.positional
        {
            return Err(error(
                "batch queries require named projections; use a single raw query for positional results",
            ));
        }
        Ok(())
    }
}

/// One result per input step, in exactly the same order. Query results should be
/// read by alias/FromQueryResult; D1 batch responses do not contain column order.
#[derive(Debug)]
pub enum BatchResult {
    Executed(ExecResult),
    Rows(Vec<QueryResult>),
}

/// Portable ordered batches over native SeaORM connections and Workers D1.
///
/// Native connections use a repeatable-read transaction (SQLite uses its native
/// transaction snapshot); D1 uses one batch call. A SQL failure rolls back the
/// batch. A zero-row write is successful SQL, not a conflict/rollback signal.
///
/// No automatic splitting, retrying, or interactive Rust transaction callback.
/// Dialects and projection modes are checked before dispatch. Errors after
/// dispatch/commit, including D1 result decoding, do not prove rollback.
#[async_trait::async_trait]
pub trait BatchConnectionTrait: ConnectionTrait {
    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr>;

    /// Backend-specific known parameter cap per SQL statement; None means the
    /// caller must consult its driver/configuration, not that it is unlimited.
    fn max_bind_parameters(&self) -> Option<usize> {
        None
    }

    /// Atomic insert/update/delete batches, with per-statement affected counts.
    async fn atomic_batch(&self, statements: &[Statement]) -> Result<Vec<ExecResult>, DbErr> {
        let steps = statements
            .iter()
            .cloned()
            .map(BatchStatement::Execute)
            .collect::<Vec<_>>();
        self.batch(&steps)
            .await?
            .into_iter()
            .map(|result| match result {
                BatchResult::Executed(result) => Ok(result),
                BatchResult::Rows(_) => Err(error("batch returned rows for an execution step")),
            })
            .collect()
    }

    /// Batched named queries. Each query has its own projection and result set;
    /// a query matching no rows contributes an empty set, not a missing entry.
    async fn query_batch(&self, queries: &[BatchQuery]) -> Result<Vec<Vec<QueryResult>>, DbErr> {
        let steps = queries
            .iter()
            .cloned()
            .map(BatchStatement::Query)
            .collect::<Vec<_>>();
        self.batch(&steps)
            .await?
            .into_iter()
            .map(|result| match result {
                BatchResult::Rows(rows) => Ok(rows),
                BatchResult::Executed(_) => {
                    Err(error("batch returned execution metadata for a query step"))
                }
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl BatchConnectionTrait for DatabaseConnection {
    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        if statements.is_empty() {
            return Ok(Vec::new());
        }
        for step in statements {
            step.validate(self.get_database_backend())?;
        }
        let transaction = self
            .begin_with_config(Some(IsolationLevel::RepeatableRead), None)
            .await?;
        let result = run_native(&transaction, statements).await;
        match result {
            Ok(results) => {
                transaction.commit().await?;
                Ok(results)
            }
            Err(cause) => {
                transaction.rollback().await.map_err(|rollback| {
                    error(format!(
                        "batch failed: {cause}; rollback failed: {rollback}"
                    ))
                })?;
                Err(cause)
            }
        }
    }
}

async fn run_native<C: ConnectionTrait>(
    connection: &C,
    statements: &[BatchStatement],
) -> Result<Vec<BatchResult>, DbErr> {
    let mut results = Vec::with_capacity(statements.len());
    for step in statements {
        results.push(match step {
            BatchStatement::Execute(statement) => {
                BatchResult::Executed(connection.execute_raw(statement.clone()).await?)
            }
            BatchStatement::Query(query) => {
                BatchResult::Rows(connection.query_all_raw(query.statement.clone()).await?)
            }
        });
    }
    Ok(results)
}
