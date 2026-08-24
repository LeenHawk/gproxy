use sea_query::{Alias, ColumnDef, Expr, Index, SqliteQueryBuilder, Table};

use super::{ColumnKind, SchemaVersion, TableSpec, tables};

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
        statements.push(create_table(table));
        statements.extend(table.indexes.iter().map(|index| {
            let mut statement = Index::create();
            statement
                .name(index.name)
                .table(Alias::new(table.name))
                .if_not_exists();
            for column in index.columns {
                statement.col(Alias::new(*column));
            }
            if index.unique {
                statement.unique();
            }
            statement.to_string(SqliteQueryBuilder)
        }));
    }
    statements
}

fn create_table(spec: &TableSpec) -> String {
    let mut table = Table::create();
    table.table(Alias::new(spec.name)).if_not_exists();
    for column in spec.columns {
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
        table.col(&mut definition);
    }
    table.to_string(SqliteQueryBuilder)
}
