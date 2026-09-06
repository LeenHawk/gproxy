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
    let supported = [
        "providers",
        "credentials",
        "routes",
        "route_members",
        "aliases",
        "price_rules",
        "price_rule_rates",
        "orgs",
        "teams",
        "users",
        "user_keys",
        "provider_models",
        "quotas",
        "routing_rules",
        "rule_sets",
        "rules",
        "provider_rule_sets",
        "instance_settings",
        "usages",
        "schema_migrations",
        "downstream_requests",
        "upstream_requests",
        "audit_logs",
    ];
    let mut query = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .map_err(error)?;
    let tables = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(error)?;
    for table in tables {
        let table = table.map_err(error)?;
        if supported.contains(&table.as_str()) {
            continue;
        }
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM \"{}\")",
            table.replace('"', "\"\"")
        );
        let populated: bool = connection
            .query_row(&sql, [], |row| row.get(0))
            .map_err(error)?;
        if populated {
            return Err(error(format!(
                "table {table} contains data not covered by automatic migration; an explicit migration review is required"
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_target(source: &Path, target: &Path) -> Result<(), AppError> {
    let target =
        Connection::open_with_flags(target, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(error)?;
    target.busy_timeout(Duration::from_secs(5)).map_err(error)?;
    target
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
        .map_err(error)?;
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
