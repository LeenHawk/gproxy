use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::store::persistence::migrations::MigrationDialect;

pub(super) async fn run(
    conn: &DatabaseConnection,
    dialect: MigrationDialect,
) -> anyhow::Result<()> {
    let cols = super::table_columns(conn, dialect, "quotas").await?;
    if cols.is_empty() {
        return Ok(());
    }
    let decimal_type = match dialect {
        MigrationDialect::Postgres => "VARCHAR(64)",
        MigrationDialect::Sqlite | MigrationDialect::MySql => "TEXT",
    };
    let integer_type = match dialect {
        MigrationDialect::Sqlite => "INTEGER",
        MigrationDialect::Postgres | MigrationDialect::MySql => "BIGINT",
    };
    for (column, definition) in [
        ("quota_daily", decimal_type.to_owned()),
        ("quota_weekly", decimal_type.to_owned()),
        ("quota_monthly", decimal_type.to_owned()),
        ("quota_5h", decimal_type.to_owned()),
        ("quota_7d", decimal_type.to_owned()),
        ("day_used", format!("{decimal_type} NOT NULL DEFAULT '0'")),
        ("day_anchor", format!("{integer_type} NOT NULL DEFAULT 0")),
        ("week_used", format!("{decimal_type} NOT NULL DEFAULT '0'")),
        ("week_anchor", format!("{integer_type} NOT NULL DEFAULT 0")),
        ("month_used", format!("{decimal_type} NOT NULL DEFAULT '0'")),
        ("month_anchor", format!("{integer_type} NOT NULL DEFAULT 0")),
        ("five_hour_used", format!("{decimal_type} NOT NULL DEFAULT '0'")),
        ("five_hour_anchor", format!("{integer_type} NOT NULL DEFAULT 0")),
        ("seven_day_used", format!("{decimal_type} NOT NULL DEFAULT '0'")),
        ("seven_day_anchor", format!("{integer_type} NOT NULL DEFAULT 0")),
    ] {
        if !cols.contains(column) {
            conn.execute_unprepared(&format!(
                "ALTER TABLE quotas ADD COLUMN {column} {definition}"
            ))
            .await?;
        }
    }
    Ok(())
}
