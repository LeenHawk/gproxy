//! Database persistence backend (SeaORM).

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

mod entities;
mod impl_backend;
mod ops;
mod schema;

/// Persistence backend backed by a SeaORM-managed database.
///
/// Supports SQLite, PostgreSQL, and MySQL. Connect with
/// [`connect`](DbPersistence::connect), passing a standard connection DSN.
/// SeaORM is an internal detail of this backend; domain code uses only the
/// [`PersistenceBackend`](super::PersistenceBackend) trait.
pub struct DbPersistence {
    conn: DatabaseConnection,
}

impl DbPersistence {
    /// Connect to the database identified by `dsn` and ensure the schema exists.
    pub async fn connect(dsn: &str) -> anyhow::Result<Self> {
        let opts = ConnectOptions::new(dsn.to_string());
        let conn = Database::connect(opts)
            .await
            .map_err(|e| anyhow::anyhow!("db connect failed: {e}"))?;
        schema::create_all(&conn).await?;
        schema::run_migrations(&conn).await?;
        Ok(Self { conn })
    }

    /// Close the underlying connection pool, flushing and releasing the SQLite
    /// file handle. MIGRATE-V1 (remove in 2.1): the v1→v2 migration builds a
    /// throwaway db here, imports into it, then must close it before renaming the
    /// file into place — an open WAL pool would otherwise hold the file.
    pub async fn close(self) -> anyhow::Result<()> {
        self.conn
            .close()
            .await
            .map_err(|e| anyhow::anyhow!("db close failed: {e}"))
    }
}

#[cfg(test)]
mod tests;
