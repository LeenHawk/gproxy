//! PostgreSQL schema selection scoped to each transaction. No session setting
//! survives a query, so transaction-mode pools can move the next transaction
//! to another server connection without changing its schema.

use std::{future::Future, pin::Pin, sync::Arc};

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr, ExecResult,
    IsolationLevel, QueryResult, SchemaBuilder, Statement, TransactionError, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;

use crate::{
    BatchConnectionTrait, BatchQuery, BatchResult, BatchStatement, SchemaSyncConnectionTrait,
    SyncReport, batch::run_native, error,
};

#[derive(Clone, Debug)]
pub struct PostgresSchemaConnection {
    pool: DatabaseConnection,
    select_schema: Arc<str>,
}

impl PostgresSchemaConnection {
    pub fn new(pool: DatabaseConnection, schema: &str) -> Result<Self, DbErr> {
        if pool.get_database_backend() != DbBackend::Postgres {
            return Err(error("schema-scoped connection requires PostgreSQL"));
        }
        Ok(Self {
            pool,
            select_schema: format!(
                "SET LOCAL search_path = \"{}\"",
                schema.replace('"', "\"\"")
            )
            .into(),
        })
    }

    async fn scoped<T, F>(
        &self,
        isolation: Option<IsolationLevel>,
        operation: F,
    ) -> Result<T, DbErr>
    where
        T: Send + 'static,
        F: for<'a> FnOnce(
                &'a DatabaseTransaction,
            )
                -> Pin<Box<dyn Future<Output = Result<T, DbErr>> + Send + 'a>>
            + Send
            + 'static,
    {
        let select_schema = self.select_schema.clone();
        self.pool
            .transaction_with_config(
                move |transaction| {
                    Box::pin(async move {
                        transaction.execute_unprepared(&select_schema).await?;
                        operation(transaction).await
                    })
                },
                isolation,
                None,
            )
            .await
            .map_err(|cause| match cause {
                TransactionError::Connection(error) | TransactionError::Transaction(error) => error,
            })
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for PostgresSchemaConnection {
    fn get_database_backend(&self) -> DbBackend {
        DbBackend::Postgres
    }
    fn support_returning(&self) -> bool {
        true
    }

    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.execute_raw(statement).await })
        })
        .await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        let sql = sql.to_owned();
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.execute_unprepared(&sql).await })
        })
        .await
    }

    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.query_one_raw(statement).await })
        })
        .await
    }

    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.query_all_raw(statement).await })
        })
        .await
    }
}

#[async_trait::async_trait]
impl BatchConnectionTrait for PostgresSchemaConnection {
    fn requires_query_projection(&self) -> bool {
        false
    }

    async fn query_rows(&self, query: BatchQuery) -> Result<Vec<QueryResult>, DbErr> {
        if query.statement.db_backend != DbBackend::Postgres {
            return Err(error("query dialect does not match the connection"));
        }
        self.query_all_raw(query.statement).await
    }

    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        if statements.is_empty() {
            return Ok(Vec::new());
        }
        for step in statements {
            step.validate(DbBackend::Postgres)?;
        }
        let statements = statements.to_vec();
        self.scoped(Some(IsolationLevel::RepeatableRead), move |tx| {
            Box::pin(async move { run_native(tx, statements.into_iter()).await })
        })
        .await
    }
}

#[async_trait::async_trait]
impl SchemaSyncConnectionTrait for PostgresSchemaConnection {
    type Registry = SchemaBuilder;

    fn schema_registry(&self) -> Self::Registry {
        self.pool.schema_registry()
    }

    async fn apply_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.apply_schema(registry).await })
        })
        .await
    }

    async fn sync_schema(&self, registry: Self::Registry) -> Result<SyncReport, DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.sync_schema(registry).await })
        })
        .await
    }

    async fn table_names(&self) -> Result<Vec<String>, DbErr> {
        self.scoped(None, |tx| Box::pin(async move { tx.table_names().await }))
            .await
    }

    async fn ledger_versions(&self, table: &str) -> Result<Vec<String>, DbErr> {
        let table = table.to_owned();
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.ledger_versions(&table).await })
        })
        .await
    }

    async fn run_migrations<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        self.scoped(None, move |tx| {
            Box::pin(async move { tx.run_migrations::<M>(steps).await })
        })
        .await
    }
}
