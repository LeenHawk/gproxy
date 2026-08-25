use sea_query::{Alias, ColumnDef, Expr, Query, SqliteQueryBuilder, Table};

use crate::StoreError;
use crate::backend::{Executor, Statement};
use crate::schema::{Dialect, SchemaVersion, migration_statements};

pub(crate) async fn migrate(executor: &dyn Executor, dialect: Dialect) -> Result<(), StoreError> {
    migrate_to(executor, dialect, SchemaVersion::LATEST).await
}

pub(crate) async fn migrate_to(
    executor: &dyn Executor,
    dialect: Dialect,
    target: SchemaVersion,
) -> Result<(), StoreError> {
    executor.execute(migration_table()).await?;
    let applied = applied_versions(executor).await?;
    for (index, version) in applied.iter().enumerate() {
        if *version != index as i64 + 1 {
            return Err(StoreError::Database(
                "schema migration history is not contiguous".into(),
            ));
        }
    }
    if applied.last().copied().unwrap_or_default() > target.number() {
        return Err(StoreError::Database(
            "database schema is newer than this binary".into(),
        ));
    }
    for version in SchemaVersion::ALL {
        if version.number() <= applied.last().copied().unwrap_or_default()
            || version.number() > target.number()
        {
            continue;
        }
        let mut statements = migration_statements(version, dialect);
        if statements
            .first()
            .is_some_and(|statement| statement.starts_with("PRAGMA "))
        {
            executor
                .execute(Statement::plain(statements.remove(0)))
                .await?;
        }
        let mut statements: Vec<_> = statements.into_iter().map(Statement::plain).collect();
        statements.push(record_version(version)?);
        executor.batch(statements).await?;
    }
    Ok(())
}

fn migration_table() -> Statement {
    let sql = Table::create()
        .table(Alias::new("schema_migrations"))
        .if_not_exists()
        .col(
            ColumnDef::new(Alias::new("version"))
                .integer()
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(Alias::new("applied_at"))
                .integer()
                .not_null(),
        )
        .to_owned()
        .to_string(SqliteQueryBuilder);
    Statement::plain(sql)
}

async fn applied_versions(executor: &dyn Executor) -> Result<Vec<i64>, StoreError> {
    let query = Query::select()
        .column(Alias::new("version"))
        .from(Alias::new("schema_migrations"))
        .order_by(Alias::new("version"), sea_query::Order::Asc)
        .to_owned();
    let result = executor.execute(Statement::query(&query)?).await?;
    result.rows.iter().map(|row| row.i64("version")).collect()
}

fn record_version(version: SchemaVersion) -> Result<Statement, StoreError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .as_secs() as i64;
    let statement = Query::insert()
        .into_table(Alias::new("schema_migrations"))
        .columns([Alias::new("version"), Alias::new("applied_at")])
        .values_panic([Expr::value(version.number()), Expr::value(now)])
        .to_owned();
    Statement::query(&statement)
}
