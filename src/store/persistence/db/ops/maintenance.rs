use anyhow::Context;
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement, TransactionTrait};

use crate::store::persistence::traits::StoragePruneResult;

const DELETE_BATCH_SIZE: u64 = 5_000;

#[derive(Clone, Copy)]
struct SqlitePages {
    count: u64,
    free: u64,
    size: u64,
}

impl SqlitePages {
    fn allocated_bytes(self) -> u64 {
        self.count.saturating_mul(self.size)
    }

    fn live_bytes(self) -> u64 {
        self.count
            .saturating_sub(self.free)
            .saturating_mul(self.size)
    }
}

pub async fn prune_observability_storage(
    conn: &DatabaseConnection,
    max_bytes: u64,
    target_bytes: u64,
) -> anyhow::Result<Option<StoragePruneResult>> {
    if conn.get_database_backend() != DatabaseBackend::Sqlite || max_bytes == 0 {
        return Ok(None);
    }

    checkpoint_wal(conn).await?;
    let before = sqlite_pages(conn).await?.allocated_bytes();
    if before <= max_bytes {
        return Ok(None);
    }

    let mut removed_rows = 0;
    let exhausted = loop {
        if sqlite_pages(conn).await?.live_bytes() <= target_bytes {
            break false;
        }

        let removed = delete_oldest_batch(conn).await?;
        removed_rows += removed;
        if removed == 0 {
            break true;
        }
    };

    // DELETE only adds free pages to SQLite's freelist. VACUUM is required for
    // the configured limit to represent the compacted database size.
    conn.execute_unprepared("VACUUM").await?;
    checkpoint_wal(conn).await?;
    let after = sqlite_pages(conn).await?.allocated_bytes();

    Ok(Some(StoragePruneResult {
        before_bytes: before,
        after_bytes: after,
        removed_rows,
        exhausted,
    }))
}

async fn checkpoint_wal(conn: &DatabaseConnection) -> anyhow::Result<()> {
    let row = conn
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "PRAGMA wal_checkpoint(TRUNCATE)",
        ))
        .await?
        .context("SQLite WAL checkpoint returned no row")?;
    let busy: i64 = row.try_get("", "busy")?;
    anyhow::ensure!(busy == 0, "SQLite WAL checkpoint is busy");
    Ok(())
}

async fn sqlite_pages(conn: &DatabaseConnection) -> anyhow::Result<SqlitePages> {
    Ok(SqlitePages {
        count: pragma_u64(conn, "PRAGMA page_count", "page_count").await?,
        free: pragma_u64(conn, "PRAGMA freelist_count", "freelist_count").await?,
        size: pragma_u64(conn, "PRAGMA page_size", "page_size").await?,
    })
}

async fn pragma_u64(
    conn: &DatabaseConnection,
    sql: &'static str,
    column: &'static str,
) -> anyhow::Result<u64> {
    let row = conn
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
        .await?
        .context("SQLite PRAGMA returned no row")?;
    let value: i64 = row.try_get("", column)?;
    u64::try_from(value).context("SQLite PRAGMA returned a negative value")
}

async fn delete_oldest_batch(conn: &DatabaseConnection) -> anyhow::Result<u64> {
    let txn = conn.begin().await?;
    let mut removed = 0;
    for table in ["downstream_requests", "upstream_requests", "audit_logs"] {
        let sql = format!(
            "DELETE FROM {table} WHERE id IN (\
             SELECT id FROM {table} ORDER BY created_at ASC, id ASC LIMIT {DELETE_BATCH_SIZE})"
        );
        removed += txn.execute_unprepared(&sql).await?.rows_affected();
    }
    txn.commit().await?;
    Ok(removed)
}
