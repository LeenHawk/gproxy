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
    let integer_ty = match dialect {
        MigrationDialect::Sqlite => "INTEGER",
        MigrationDialect::Postgres | MigrationDialect::MySql => "BIGINT",
    };
    for column in ["context_window", "max_output_tokens"] {
        if !cols.contains(column) {
            conn.execute_unprepared(&format!(
                "ALTER TABLE provider_models ADD COLUMN {column} {integer_ty}"
            ))
            .await?;
        }
    }
    for column in [
        "thinking_supported",
        "thinking_adaptive_supported",
        "thinking_enabled_supported",
    ] {
        if !cols.contains(column) {
            conn.execute_unprepared(&format!(
                "ALTER TABLE provider_models ADD COLUMN {column} BOOLEAN"
            ))
            .await?;
        }
    }
    fold_max_input_tokens(conn, &cols).await?;
    Ok(())
}

/// `max_input_tokens` was merged into `context_window` — the context window
/// *is* the input allowance. Claude/Gemini rows kept their limit in the dropped
/// column with `context_window` NULL, so backfill before dropping or those
/// limits vanish. Lives here rather than in a [`Migration`] because the fold
/// must be conditional on the column existing: migrations run *before* repair
/// and their SQL is unconditional, so a DB already created at the current
/// baseline would fail on a bare `DROP COLUMN`.
///
/// [`Migration`]: crate::store::persistence::migrations::Migration
async fn fold_max_input_tokens(
    conn: &DatabaseConnection,
    cols: &std::collections::HashSet<String>,
) -> anyhow::Result<()> {
    if !cols.contains("max_input_tokens") {
        return Ok(());
    }
    conn.execute_unprepared(
        "UPDATE provider_models SET context_window = max_input_tokens \
         WHERE context_window IS NULL AND max_input_tokens IS NOT NULL",
    )
    .await?;
    conn.execute_unprepared("ALTER TABLE provider_models DROP COLUMN max_input_tokens")
        .await?;
    Ok(())
}
