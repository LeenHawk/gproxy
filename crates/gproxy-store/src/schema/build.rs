use sea_query::{
    Alias, ColumnDef, Expr, Index, MysqlQueryBuilder, PostgresQueryBuilder, SqliteQueryBuilder,
    Table,
};

use super::catalog::migration_tables;
use super::{ColumnKind, IndexSpec, SchemaVersion, TableSpec, tables, wave26};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    NativeSqlite,
    Libsql,
    Postgres,
    Mysql,
}

pub fn migration_statements(version: SchemaVersion, dialect: Dialect) -> Vec<String> {
    let mut statements = Vec::new();
    if version == SchemaVersion::Control && dialect == Dialect::NativeSqlite {
        statements.push("PRAGMA foreign_keys = ON".to_owned());
    }
    for table in migration_tables().filter(|table| {
        table.version == version
            && !(version == SchemaVersion::Wave26
                && matches!(table.name, "admin_audit_events" | "credential_health"))
    }) {
        statements.push(create_table(table, version, dialect));
        statements.extend(
            table
                .indexes
                .iter()
                .filter(|index| {
                    index
                        .added_in
                        .is_none_or(|added| added.number() <= version.number())
                })
                .map(|index| create_index(table.name, index, dialect)),
        );
    }
    if version == SchemaVersion::Logging {
        statements.extend(rebuild_wire_logs(version, dialect));
    }
    for table in tables().filter(|table| table.version != version) {
        if version == SchemaVersion::Logging && table.name == "wire_logs" {
            continue;
        }
        for column in table
            .columns
            .iter()
            .filter(|column| column.added_in == Some(version))
        {
            statements.push(add_column(table, column, dialect));
        }
        statements.extend(
            table
                .indexes
                .iter()
                .filter(|index| index.added_in == Some(version))
                .map(|index| create_index(table.name, index, dialect)),
        );
    }
    if version == SchemaVersion::Routing {
        statements.push("UPDATE route_members SET tier = priority".to_owned());
    }
    if version == SchemaVersion::Wave26 {
        statements.extend(wave26::statements(dialect));
    }
    if version == SchemaVersion::Wave30 {
        statements.push(
            "UPDATE tokenizer_vocabs SET repository = name WHERE repository IS NULL".to_owned(),
        );
    }
    statements
}

fn rebuild_wire_logs(version: SchemaVersion, dialect: Dialect) -> Vec<String> {
    let spec = tables()
        .find(|table| table.name == "wire_logs")
        .expect("wire log schema exists");
    let old_columns = [
        "id",
        "request_id",
        "at",
        "provider_id",
        "credential_id",
        "upstream_url",
        "response_status",
        "request_body",
        "response_body",
    ];
    let columns = old_columns.join(", ");
    let mut statements = vec![
        "ALTER TABLE wire_logs RENAME TO wire_logs_before_logging".to_owned(),
        create_table(spec, version, dialect),
        format!("INSERT INTO wire_logs ({columns}) SELECT {columns} FROM wire_logs_before_logging"),
        "DROP TABLE wire_logs_before_logging".to_owned(),
    ];
    statements.extend(
        spec.indexes
            .iter()
            .map(|index| create_index(spec.name, index, dialect)),
    );
    statements
}

pub(super) fn create_table(spec: &TableSpec, version: SchemaVersion, dialect: Dialect) -> String {
    let mut table = Table::create();
    table.table(Alias::new(spec.name)).if_not_exists();
    for column in spec.columns.iter().filter(|column| {
        column
            .added_in
            .is_none_or(|added| added.number() <= version.number())
    }) {
        let indexed = column.unique
            || column.primary_key
            || spec
                .indexes
                .iter()
                .any(|index| index.columns.contains(&column.name));
        let mut definition = column_definition(column, dialect, indexed);
        table.col(&mut definition);
    }
    match dialect {
        Dialect::NativeSqlite | Dialect::Libsql => table.to_string(SqliteQueryBuilder),
        Dialect::Postgres => table.to_string(PostgresQueryBuilder),
        Dialect::Mysql => table.to_string(MysqlQueryBuilder),
    }
}

fn add_column(spec: &TableSpec, column: &super::ColumnSpec, dialect: Dialect) -> String {
    let mut statement = Table::alter();
    let indexed = column.unique
        || column.primary_key
        || spec
            .indexes
            .iter()
            .any(|index| index.columns.contains(&column.name));
    let mut definition = column_definition(column, dialect, indexed);
    statement
        .table(Alias::new(spec.name))
        .add_column(&mut definition);
    match dialect {
        Dialect::NativeSqlite | Dialect::Libsql => statement.to_string(SqliteQueryBuilder),
        Dialect::Postgres => statement.to_string(PostgresQueryBuilder),
        Dialect::Mysql => statement.to_string(MysqlQueryBuilder),
    }
}

fn column_definition(column: &super::ColumnSpec, dialect: Dialect, indexed: bool) -> ColumnDef {
    let mut definition = ColumnDef::new(Alias::new(column.name));
    match column.kind {
        ColumnKind::Integer if matches!(dialect, Dialect::Postgres | Dialect::Mysql) => {
            definition.big_integer()
        }
        ColumnKind::Integer => definition.integer(),
        ColumnKind::Text if dialect == Dialect::Mysql && indexed => definition.string_len(255),
        ColumnKind::Text => definition.text(),
        ColumnKind::Blob if dialect == Dialect::Mysql && indexed => definition.var_binary(255),
        ColumnKind::Blob if dialect == Dialect::Mysql => definition.blob(),
        ColumnKind::Blob => definition.binary(),
    };
    if !column.nullable {
        definition.not_null();
    }
    if column.primary_key {
        definition.primary_key();
    }
    if column.auto_increment {
        definition.auto_increment();
    }
    if column.unique {
        definition.unique_key();
    }
    if let Some(default) = column.default {
        definition.default(Expr::cust(default));
    }
    definition
}

pub(super) fn create_index(table: &str, index: &IndexSpec, dialect: Dialect) -> String {
    let mut statement = Index::create();
    statement
        .name(index.name)
        .table(Alias::new(table))
        .if_not_exists();
    for column in index.columns {
        statement.col(Alias::new(*column));
    }
    if index.unique {
        statement.unique();
    }
    match dialect {
        Dialect::NativeSqlite | Dialect::Libsql => statement.to_string(SqliteQueryBuilder),
        Dialect::Postgres => statement.to_string(PostgresQueryBuilder),
        Dialect::Mysql => statement.to_string(MysqlQueryBuilder),
    }
}
