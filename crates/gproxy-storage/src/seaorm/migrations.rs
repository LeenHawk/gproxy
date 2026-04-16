//! Idempotent, multi-backend schema migrations applied before `schema.sync()`.
//!
//! Each migration must be safe to run on a fresh DB (no-op) and on an old DB
//! (applies the change once). We don't have a migration ledger — safety is
//! achieved by inspecting the current schema and only issuing changes when
//! the old shape is detected.

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement};

/// Run all pre-sync migrations. Call this before `schema.sync(&db)`.
pub(crate) async fn run_pre_sync(db: &DatabaseConnection) -> Result<(), DbErr> {
    rename_providers_dispatch_json_to_routing_json(db).await?;
    Ok(())
}

/// Rename `providers.dispatch_json` → `providers.routing_json` if the old
/// column still exists. Idempotent across SQLite, MySQL, and PostgreSQL.
async fn rename_providers_dispatch_json_to_routing_json(
    db: &DatabaseConnection,
) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let has_old = column_exists(db, backend, "providers", "dispatch_json").await?;
    if !has_old {
        return Ok(());
    }
    let has_new = column_exists(db, backend, "providers", "routing_json").await?;
    if has_new {
        // Both columns present — abnormal state, skip rather than destroy data.
        return Ok(());
    }
    let sql = match backend {
        DatabaseBackend::Sqlite => {
            "ALTER TABLE providers RENAME COLUMN dispatch_json TO routing_json"
        }
        DatabaseBackend::MySql => {
            "ALTER TABLE providers RENAME COLUMN dispatch_json TO routing_json"
        }
        DatabaseBackend::Postgres => {
            "ALTER TABLE providers RENAME COLUMN dispatch_json TO routing_json"
        }
        _ => "ALTER TABLE providers RENAME COLUMN dispatch_json TO routing_json",
    };
    db.execute_unprepared(sql).await?;
    Ok(())
}

async fn column_exists(
    db: &DatabaseConnection,
    backend: DatabaseBackend,
    table: &str,
    column: &str,
) -> Result<bool, DbErr> {
    let (sql, values): (&str, Vec<sea_orm::Value>) = match backend {
        DatabaseBackend::Sqlite => {
            // PRAGMA doesn't support bind params; use format_pragma via execute_unprepared path
            let pragma = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));
            let stmt = Statement::from_string(backend, pragma);
            let rows = db.query_all_raw(stmt).await?;
            for row in rows {
                let name: String = row.try_get("", "name").unwrap_or_default();
                if name == column {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        DatabaseBackend::MySql => (
            "SELECT COUNT(*) as cnt FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND COLUMN_NAME = ?",
            vec![table.into(), column.into()],
        ),
        DatabaseBackend::Postgres => (
            "SELECT COUNT(*) as cnt FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2",
            vec![table.into(), column.into()],
        ),
        _ => (
            "SELECT COUNT(*) as cnt FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 AND column_name = $2",
            vec![table.into(), column.into()],
        ),
    };
    let stmt = Statement::from_sql_and_values(backend, sql, values);
    let row = db.query_one_raw(stmt).await?;
    match row {
        Some(r) => {
            let cnt: i64 = r.try_get("", "cnt").unwrap_or(0);
            Ok(cnt > 0)
        }
        None => Ok(false),
    }
}
