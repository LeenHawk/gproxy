use std::path::Path;
use std::time::Duration;

use rust_decimal::Decimal;
use tokio_rusqlite::rusqlite::{Connection, OpenFlags};

use super::{AppError, error};

pub(super) fn is_v2(path: &Path) -> Result<bool, AppError> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(error)?;
    let column = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('user_keys') WHERE name='api_key_ciphertext'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(error)?;
    Ok(column > 0)
}

pub(super) fn quiesce(path: &Path) -> Result<Connection, AppError> {
    let connection =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(error)?;
    connection.execute_batch("PRAGMA locking_mode=EXCLUSIVE; PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE;")
        .map_err(|_| error("cannot exclusively lock the v2 database; stop all processes using it before upgrading"))?;
    check(&connection)?;
    Ok(connection)
}

pub(super) fn validate_source(connection: &Connection) -> Result<(), AppError> {
    let (_, issues) = super::super::tables::inspect(connection).map_err(error)?;
    if !issues.is_empty() {
        return Err(error(
            issues
                .iter()
                .map(|issue| format!("table {}: {}", issue.row, issue.reason))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(())
}

pub(super) fn snapshot_target(source: &Path, target: &Path) -> Result<(), AppError> {
    let connection =
        Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(error)?;
    connection
        .execute(
            "VACUUM INTO ?1",
            [target
                .to_str()
                .ok_or_else(|| error("upgrade path is not UTF-8"))?],
        )
        .map_err(error)?;
    Ok(())
}

pub(super) fn validate_target(source: &Path, target: &Path) -> Result<(), AppError> {
    let target =
        Connection::open_with_flags(target, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(error)?;
    target.busy_timeout(Duration::from_secs(5)).map_err(error)?;
    check(&target)?;
    let source =
        Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(error)?;
    if usage(&source, "usages")? != usage(&target, "usage_rows")? {
        return Err(error(
            "usage row count or total settled cost changed during migration",
        ));
    }
    let violations: i64 = target
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(error)?;
    if violations != 0 {
        return Err(error("migrated database has invalid references"));
    }
    Ok(())
}

fn check(connection: &Connection) -> Result<(), AppError> {
    let status: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(error)?;
    if status != "ok" {
        return Err(error("SQLite integrity check failed"));
    }
    Ok(())
}

fn usage(connection: &Connection, table: &str) -> Result<(u64, Decimal), AppError> {
    let mut query = connection
        .prepare(&format!("SELECT cost FROM {table}"))
        .map_err(error)?;
    let mut rows = query.query([]).map_err(error)?;
    let mut count = 0;
    let mut cost = Decimal::ZERO;
    while let Some(row) = rows.next().map_err(error)? {
        let amount = row
            .get::<_, String>(0)
            .map_err(error)?
            .parse::<Decimal>()
            .map_err(error)?;
        cost = cost
            .checked_add(amount)
            .ok_or_else(|| error("settled cost total overflows"))?;
        count += 1;
    }
    Ok((count, cost))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutover_snapshot_is_standalone_while_the_import_connection_is_still_open() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("candidate.db");
        let target = directory.path().join("ready.db");
        let importing = Connection::open(&source).unwrap();
        importing.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE usage_rows(cost TEXT); INSERT INTO usage_rows VALUES('1.234'); BEGIN; SELECT cost FROM usage_rows;").unwrap();
        snapshot_target(&source, &target).unwrap();
        let ready = Connection::open_with_flags(&target, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            usage(&ready, "usage_rows").unwrap(),
            (1, "1.234".parse().unwrap())
        );
        assert_eq!(
            ready
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "delete"
        );
        assert!(!directory.path().join("ready.db-wal").exists());
        check(&ready).unwrap();
    }
}
