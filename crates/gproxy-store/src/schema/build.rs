use sea_query::{Alias, ColumnDef, Expr, Index, SqliteQueryBuilder, Table};

use super::{ColumnKind, IndexSpec, SchemaVersion, TableSpec, tables};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    NativeSqlite,
    Libsql,
}

pub fn migration_statements(version: SchemaVersion, dialect: Dialect) -> Vec<String> {
    let mut statements = Vec::new();
    if version == SchemaVersion::Control && dialect == Dialect::NativeSqlite {
        statements.push("PRAGMA foreign_keys = ON".to_owned());
    }
    for table in tables().filter(|table| table.version == version) {
        statements.push(create_table(table, version));
        statements.extend(
            table
                .indexes
                .iter()
                .filter(|index| {
                    index
                        .added_in
                        .is_none_or(|added| added.number() <= version.number())
                })
                .map(|index| create_index(table.name, index)),
        );
    }
    if version == SchemaVersion::Logging {
        statements.extend(rebuild_wire_logs(version));
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
            statements.push(add_column(table, column));
        }
        statements.extend(
            table
                .indexes
                .iter()
                .filter(|index| index.added_in == Some(version))
                .map(|index| create_index(table.name, index)),
        );
    }
    if version == SchemaVersion::Routing {
        statements.push("UPDATE route_members SET tier = priority".to_owned());
    }
    statements
}

fn rebuild_wire_logs(version: SchemaVersion) -> Vec<String> {
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
        create_table(spec, version),
        format!("INSERT INTO wire_logs ({columns}) SELECT {columns} FROM wire_logs_before_logging"),
        "DROP TABLE wire_logs_before_logging".to_owned(),
    ];
    statements.extend(
        spec.indexes
            .iter()
            .map(|index| create_index(spec.name, index)),
    );
    statements
}

fn create_table(spec: &TableSpec, version: SchemaVersion) -> String {
    let mut table = Table::create();
    table.table(Alias::new(spec.name)).if_not_exists();
    for column in spec.columns.iter().filter(|column| {
        column
            .added_in
            .is_none_or(|added| added.number() <= version.number())
    }) {
        let mut definition = column_definition(column);
        table.col(&mut definition);
    }
    table.to_string(SqliteQueryBuilder)
}

fn add_column(spec: &TableSpec, column: &super::ColumnSpec) -> String {
    let mut statement = Table::alter();
    let mut definition = column_definition(column);
    statement
        .table(Alias::new(spec.name))
        .add_column(&mut definition);
    statement.to_string(SqliteQueryBuilder)
}

fn column_definition(column: &super::ColumnSpec) -> ColumnDef {
    let mut definition = ColumnDef::new(Alias::new(column.name));
    match column.kind {
        ColumnKind::Integer => definition.integer(),
        ColumnKind::Text => definition.text(),
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

fn create_index(table: &str, index: &IndexSpec) -> String {
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
    statement.to_string(SqliteQueryBuilder)
}
