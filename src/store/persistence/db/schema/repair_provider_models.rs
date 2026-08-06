use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::store::persistence::migrations::MigrationDialect;

pub(super) async fn run(
    conn: &DatabaseConnection,
    dialect: MigrationDialect,
) -> anyhow::Result<()> {
    let cols = super::table_columns(conn, dialect, "provider_models").await?;
    if cols.is_empty() {
        return Ok(());
    }
    let ty = match dialect {
        MigrationDialect::Sqlite => "INTEGER",
        MigrationDialect::Postgres | MigrationDialect::MySql => "BIGINT",
    };
    for column in ["context_window", "max_input_tokens", "max_output_tokens"] {
        if !cols.contains(column) {
            conn.execute_unprepared(&format!(
                "ALTER TABLE provider_models ADD COLUMN {column} {ty}"
            ))
            .await?;
        }
    }
    Ok(())
}
