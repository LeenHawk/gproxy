use sea_query::{Alias, MysqlQueryBuilder, PostgresQueryBuilder, SchemaBuilder, Table};

use super::{Dialect, SchemaVersion, TableSpec};

pub(super) fn alter(table: &TableSpec, version: SchemaVersion, dialect: Dialect) -> Vec<String> {
    match dialect {
        Dialect::NativeSqlite | Dialect::Libsql => rebuild(table, version, dialect),
        Dialect::Postgres => columns(table, version, dialect, PostgresQueryBuilder),
        Dialect::Mysql => columns(table, version, dialect, MysqlQueryBuilder),
    }
}

fn columns<B: SchemaBuilder>(
    table: &TableSpec,
    version: SchemaVersion,
    dialect: Dialect,
    builder: B,
) -> Vec<String> {
    table
        .columns
        .iter()
        .filter(|column| column.nullable_in == Some(version))
        .map(|column| {
            let mut definition = super::build::column_definition(column, dialect, false);
            definition.null();
            let statement = Table::alter()
                .table(Alias::new(table.name))
                .modify_column(definition)
                .to_owned();
            let mut sql = String::new();
            builder.prepare_table_alter_statement(&statement, &mut sql);
            sql
        })
        .collect()
}

fn rebuild(table: &TableSpec, version: SchemaVersion, dialect: Dialect) -> Vec<String> {
    let temporary = format!("{}_before_nullable", table.name);
    let names = table
        .columns
        .iter()
        .filter(|column| {
            column
                .added_in
                .is_none_or(|added| added.number() < version.number())
        })
        .map(|column| format!("\"{}\"", column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut statements = vec![
        format!("ALTER TABLE \"{}\" RENAME TO \"{temporary}\"", table.name),
        super::build::create_table(table, version, dialect),
        format!(
            "INSERT INTO \"{}\" ({names}) SELECT {names} FROM \"{temporary}\"",
            table.name
        ),
        format!("DROP TABLE \"{temporary}\""),
    ];
    statements.extend(
        table
            .indexes
            .iter()
            .filter(|index| {
                index
                    .added_in
                    .is_none_or(|added| added.number() <= version.number())
            })
            .map(|index| super::build::create_index(table.name, index, dialect)),
    );
    statements
}
