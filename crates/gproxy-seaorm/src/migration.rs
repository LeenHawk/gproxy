use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sea_orm::{
    ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, ProxyDatabaseTrait,
    ProxyExecResult, ProxyRow, Statement,
};
use sea_orm_migration::{MigrationStatus, MigratorTrait};

use crate::{D1Connection, D1Type, Projection, codec, error};

/// Private bridge used only inside the official migration runner. Never expose
/// its DatabaseConnection as a supposedly transactional application connection.
#[derive(Debug)]
struct MigrationProxy {
    connection: D1Connection,
    invalid_transaction: Arc<AtomicBool>,
}

impl MigrationProxy {
    fn check(&self) -> Result<(), DbErr> {
        if self.invalid_transaction.load(Ordering::SeqCst) {
            Err(error(
                "D1 migrations cannot use interactive begin/commit/rollback",
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait::async_trait]
impl ProxyDatabaseTrait for MigrationProxy {
    async fn query(&self, statement: Statement) -> Result<Vec<ProxyRow>, DbErr> {
        self.check()?;
        let result = self.connection.raw_rows(statement).await?;
        Ok(codec::rows(&result, &self.connection.projection)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
    async fn execute(&self, statement: Statement) -> Result<ProxyExecResult, DbErr> {
        self.check()?;
        Ok(self.connection.execute_raw(statement).await?.into())
    }
    // SeaORM's Proxy hooks cannot return errors. Poison the bridge on entry so
    // no subsequent statement or migration-history write can falsely succeed.
    async fn begin(&self) {
        self.invalid_transaction.store(true, Ordering::SeqCst);
    }
    async fn commit(&self) {
        self.invalid_transaction.store(true, Ordering::SeqCst);
    }
    async fn rollback(&self) {
        self.invalid_transaction.store(true, Ordering::SeqCst);
    }
    fn start_rollback(&self) {
        self.invalid_transaction.store(true, Ordering::SeqCst);
    }
}

impl D1Connection {
    async fn migration_bridge(&self) -> Result<(DatabaseConnection, Arc<AtomicBool>), DbErr> {
        let projection = Projection::for_entity::<sea_orm_migration::seaql_migrations::Entity>()?
            .column("__d1_exists", D1Type::Bool, false)?
            .merge(self.projection.clone())?;
        let invalid = Arc::new(AtomicBool::new(false));
        let proxy = MigrationProxy {
            connection: self.with_projection(projection),
            invalid_transaction: Arc::clone(&invalid),
        };
        let db = Database::connect_proxy(DbBackend::Sqlite, Arc::new(Box::new(proxy))).await?;
        Ok((db, invalid))
    }

    /// Run the official SeaORM migrator. D1 follows SeaORM's default SQLite
    /// semantics: statements auto-commit, and the migration record is written
    /// after up succeeds.
    /// A failed migration can leave partial DDL/data; repair it before retrying.
    pub async fn migrate_up<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        let (db, invalid) = self.migration_bridge().await?;
        let result = M::up(&db, steps).await;
        migration_result(result, &invalid)
    }

    /// Run official down migrations in reverse order. This deliberately permits
    /// the destructive changes authored in the caller's MigrationTrait::down.
    pub async fn migrate_down<M: MigratorTrait>(&self, steps: Option<u32>) -> Result<(), DbErr> {
        let (db, invalid) = self.migration_bridge().await?;
        let result = M::down(&db, steps).await;
        migration_result(result, &invalid)
    }

    /// Official migration status; like MigratorTrait's default status path, this
    /// initializes the migration-history table if it does not yet exist.
    pub async fn migration_status<M: MigratorTrait>(
        &self,
    ) -> Result<Vec<(String, MigrationStatus)>, DbErr> {
        let (db, invalid) = self.migration_bridge().await?;
        let result = M::get_migration_with_status(&db)
            .await?
            .into_iter()
            .map(|migration| (migration.name().to_owned(), migration.status()))
            .collect();
        migration_result(Ok(result), &invalid)
    }
}

fn migration_result<T>(result: Result<T, DbErr>, invalid: &AtomicBool) -> Result<T, DbErr> {
    if invalid.load(Ordering::SeqCst) {
        Err(error("D1 migrations cannot use interactive transactions"))
    } else {
        result
    }
}
