//! Entity-first schema creation for D1, following SeaORM's SQLite rules.
//! SeaORM generates DDL and dependency order. D1 supplies schema discovery and
//! executes the resulting statements. There is no version guard or index-ownership registry.
//!
//! The planner below still diffs a desired registry against a discovered schema,
//! because that is how the D1 adapter creates what is missing. What no longer
//! happens is a *caller* reaching for it to evolve a populated database: an
//! existing database is moved forward by a versioned migration, and the only
//! entry point the store uses here is the empty-snapshot plan — a fresh install.

use crate::error;
use sea_orm::sea_query::{
    ColumnDef, Index, IndexCreateStatement, SqliteQueryBuilder, Table, TableBuilder,
    TableCreateStatement,
};
use sea_orm::{
    ConnectionTrait, DbBackend, DbErr, EntityTrait, ExecResult, QueryResult, Schema, SchemaBuilder,
    Statement,
};
use std::collections::BTreeMap;

mod connection;
#[cfg(any(target_arch = "wasm32", feature = "libsql"))]
mod discover;
pub use connection::{
    EntityRegistry, SchemaSyncConnectionTrait, SyncReport, is_engine_table, ledger_table_name,
};
#[cfg(any(target_arch = "wasm32", feature = "libsql"))]
pub use discover::ProjectedConnection;
#[cfg(any(target_arch = "wasm32", feature = "libsql"))]
pub(crate) use discover::{apply_registry, ledger_versions, table_names};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    pub sql_type: String,
    pub not_null: bool,
    pub default_sql: Option<String>,
    pub primary_key_position: i64,
    pub hidden: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub origin: String,
    pub partial: bool,
}
#[derive(Clone, Debug, Default)]
pub struct TableInfo {
    pub columns: BTreeMap<String, ColumnInfo>,
    pub indexes: BTreeMap<String, IndexInfo>,
}
#[derive(Clone, Debug, Default)]
pub struct SchemaSnapshot {
    pub tables: BTreeMap<String, TableInfo>,
}
struct DesiredTable {
    create: TableCreateStatement,
    indexes: Vec<IndexCreateStatement>,
}
pub struct SchemaSync {
    builder: SchemaBuilder,
    tables: BTreeMap<String, DesiredTable>,
}
impl Default for SchemaSync {
    fn default() -> Self {
        Self::new()
    }
}
impl SchemaSync {
    pub fn new() -> Self {
        Self {
            builder: SchemaBuilder::new(Schema::new(DbBackend::Sqlite)),
            tables: BTreeMap::new(),
        }
    }
    /// Register an entity. Repeated registration is ignored, as in SeaORM's builder.
    pub fn register<E: EntityTrait>(mut self, entity: E) -> Self {
        let name = entity.table_name().to_owned();
        if self.tables.contains_key(&name) {
            return self;
        }
        let schema = Schema::new(DbBackend::Sqlite);
        self.tables.insert(
            name,
            DesiredTable {
                create: schema.create_table_from_entity(entity),
                indexes: schema.create_index_from_entity(entity),
            },
        );
        self.builder = self.builder.register(entity);
        self
    }
    /// Generate D1 statements from discovered schema, using SeaORM's creation order.
    pub async fn plan_snapshot(self, snapshot: &SchemaSnapshot) -> Result<SyncPlan, DbErr> {
        let recorder = Recorder::default();
        self.builder.apply(&recorder).await?;
        let creation_order = recorder
            .0
            .into_inner()
            .map_err(|_| error("schema recorder poisoned"))?;
        let mut statements = Vec::new();
        let mut warnings = Vec::new();
        for generated in creation_order {
            let Some((name, desired)) = self
                .tables
                .iter()
                .find(|(_, table)| DbBackend::Sqlite.build(&table.create).sql == generated.sql)
            else {
                continue;
            };
            if let Some(existing) = snapshot.tables.get(name) {
                plan_table(name, desired, existing, &mut statements, &mut warnings)?;
            } else {
                statements.push(generated);
                statements.extend(
                    desired
                        .indexes
                        .iter()
                        .map(|index| DbBackend::Sqlite.build(index)),
                );
            }
        }
        Ok(SyncPlan {
            statements,
            warnings,
        })
    }
}
#[derive(Debug)]
pub struct SyncPlan {
    statements: Vec<Statement>,
    /// Existing column type differences, which SeaORM sync leaves for migrations.
    pub warnings: Vec<String>,
}
impl SyncPlan {
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}
fn plan_table(
    name: &str,
    desired: &DesiredTable,
    existing: &TableInfo,
    statements: &mut Vec<Statement>,
    warnings: &mut Vec<String>,
) -> Result<(), DbErr> {
    let mut effective = existing.clone();
    for column in desired.create.get_columns() {
        let target = column.get_column_name();
        if let Some(actual) = effective.columns.get(&target) {
            let expected = type_sql(column)?;
            if !actual.sql_type.eq_ignore_ascii_case(&expected) {
                warnings.push(format!("column {name}.{target} is {}, entity defines {expected}; sync leaves existing types unchanged",actual.sql_type));
            }
            continue;
        }
        if let Some(old) = renamed_from(column) {
            statements.push(
                DbBackend::Sqlite.build(
                    Table::alter()
                        .table(name.to_owned())
                        .rename_column(old.to_owned(), target.clone()),
                ),
            );
            if let Some(mut old_column) = effective.columns.remove(old) {
                old_column.name = target.clone();
                effective.columns.insert(target.clone(), old_column);
                for index in effective.indexes.values_mut() {
                    for name in &mut index.columns {
                        if name == old {
                            *name = target.clone();
                        }
                    }
                }
            }
        } else {
            // Let SQLite apply its ADD COLUMN rules, just as SeaORM does.
            statements.push(
                DbBackend::Sqlite.build(
                    Table::alter()
                        .table(name.to_owned())
                        .add_column(column.clone()),
                ),
            );
        }
    }
    // Existing-table FK changes are skipped for SQLite, matching SeaORM sync.
    for wanted in &desired.indexes {
        let columns = wanted.get_index_spec().get_column_names();
        if !effective
            .indexes
            .values()
            .any(|index| index.columns == columns)
        {
            let mut create = wanted.clone();
            create.if_not_exists();
            statements.push(DbBackend::Sqlite.build(&create));
        }
    }
    for column in desired.create.get_columns() {
        let column_name = column.get_column_name();
        if column.get_column_spec().unique
            && effective.columns.contains_key(&column_name)
            && !effective
                .indexes
                .values()
                .any(|index| index.unique && index.columns == [column_name.clone()])
        {
            statements.push(
                DbBackend::Sqlite.build(
                    Index::create()
                        .name(format!("idx-{name}-{column_name}"))
                        .table(name.to_owned())
                        .col(column_name)
                        .unique()
                        .if_not_exists(),
                ),
            );
        }
    }
    // SeaORM removes unique indexes absent from the entity, regardless of who
    // created them. SQLite may reject removal of an inline UNIQUE auto-index;
    // that backend error is returned rather than inventing an ownership policy.
    for actual in effective
        .indexes
        .values()
        .filter(|index| index.unique && index.origin != "pk")
    {
        let declared = desired
            .indexes
            .iter()
            .any(|index| index.get_index_spec().get_column_names() == actual.columns)
            || desired.create.get_columns().iter().any(|column| {
                column.get_column_spec().unique && actual.columns == [column.get_column_name()]
            });
        if !declared {
            statements.push(DbBackend::Sqlite.build(Index::drop().name(&actual.name)));
        }
    }
    Ok(())
}
fn renamed_from(column: &ColumnDef) -> Option<&str> {
    column
        .get_column_spec()
        .comment
        .as_deref()?
        .rsplit_once("renamed_from \"")?
        .1
        .split_once('"')
        .map(|(name, _)| name)
}
fn type_sql(column: &ColumnDef) -> Result<String, DbErr> {
    let mut sql = String::new();
    TableBuilder::prepare_column_type(
        &SqliteQueryBuilder,
        column
            .get_column_type()
            .ok_or_else(|| error("missing column type"))?,
        &mut sql,
    );
    Ok(sql)
}
#[cfg(any(target_arch = "wasm32", feature = "libsql"))]
fn sql(text: &str, values: Vec<sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Sqlite, text, values)
}

#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<Statement>>);
#[async_trait::async_trait]
impl ConnectionTrait for Recorder {
    fn get_database_backend(&self) -> DbBackend {
        DbBackend::Sqlite
    }
    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        self.0
            .lock()
            .map_err(|_| error("schema recorder poisoned"))?
            .push(statement);
        #[cfg(target_arch = "wasm32")]
        return Ok(sea_orm::ProxyExecResult::default().into());
        #[cfg(not(target_arch = "wasm32"))]
        {
            use sea_orm::{MockDatabase, MockDatabaseTrait, MockExecResult};
            MockDatabase::new(DbBackend::Sqlite)
                .append_exec_results([MockExecResult::default()])
                .execute(0, Statement::from_string(DbBackend::Sqlite, ""))
        }
    }
    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await
    }
    async fn query_one_raw(&self, _: Statement) -> Result<Option<QueryResult>, DbErr> {
        Err(error("unexpected query while collecting generated DDL"))
    }
    async fn query_all_raw(&self, _: Statement) -> Result<Vec<QueryResult>, DbErr> {
        Err(error("unexpected query while collecting generated DDL"))
    }
}
