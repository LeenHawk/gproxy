use super::super::{Error, Result};
use gproxy_seaorm::{
    BatchConnectionTrait, BatchQuery, D1Type, Projection, SchemaSyncConnectionTrait,
};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Order, Query};
use sea_orm::{DbBackend, QueryResult, Statement};
use std::collections::BTreeMap;

pub struct Source<'a, C> {
    pub connection: &'a C,
    pub tables: Vec<String>,
    prefix: &'a str,
    columns: BTreeMap<String, Vec<(String, String)>>,
}

impl<'a, C: BatchConnectionTrait + SchemaSyncConnectionTrait> Source<'a, C> {
    pub async fn new(connection: &'a C, prefix: &'a str) -> Result<Self> {
        let tables = connection
            .table_names()
            .await?
            .into_iter()
            .filter_map(|name| name.strip_prefix(prefix).map(str::to_owned))
            .collect();
        let mut columns: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        if connection.requires_query_projection() {
            // One metadata query for the whole source, not a pragma per row
            // batch. Avoid repeating metadata reads on every usage page.
            let rows = connection.query_rows(BatchQuery::new(
                Statement::from_string(DbBackend::Sqlite,
                    "SELECT m.name AS table_name, p.name, p.type FROM sqlite_schema AS m JOIN pragma_table_info(m.name) AS p WHERE m.type='table' AND substr(m.name,1,7)<>'sqlite_' AND substr(m.name,1,4)<>'_cf_'"),
                Projection::new().column("table_name", D1Type::Text, false)?
                    .column("name", D1Type::Text, false)?.column("type", D1Type::Text, false)?,
            )).await?;
            for row in rows {
                let table: String = row.try_get("", "table_name")?;
                columns
                    .entry(table)
                    .or_default()
                    .push((row.try_get("", "name")?, row.try_get("", "type")?));
            }
        }
        Ok(Self {
            connection,
            tables,
            prefix,
            columns,
        })
    }
}

impl<C: BatchConnectionTrait> Source<'_, C> {
    pub async fn rows(
        &self,
        table: &str,
        order: &[&str],
        after: Option<(&str, i64)>,
        limit: Option<u64>,
    ) -> Result<Vec<QueryResult>> {
        let name = format!("{}{table}", self.prefix);
        let backend = self.connection.get_database_backend();
        let mut query = Query::select();
        let mut projection = Projection::new();
        if self.connection.requires_query_projection() {
            // D1/libSQL return untyped JS values. Cast integers to text before
            // transport so v3 identifiers above 2^53 keep their exact value.
            let columns = self
                .columns
                .get(&name)
                .ok_or_else(|| Error::other(format!("v3 table {name} has no column metadata")))?;
            for (column, sql_type) in columns {
                let kind = match sql_type.to_ascii_uppercase().as_str() {
                    "INTEGER" | "INT" | "BIGINT" => D1Type::I64,
                    "BLOB" | "BINARY" => D1Type::Bytes,
                    "TEXT" | "VARCHAR" | "CLOB" => D1Type::Text,
                    other if other.starts_with("VARCHAR(") => D1Type::Text,
                    other => {
                        return Err(Error::other(format!(
                            "unsupported v3 column type {table}.{column}: {other}"
                        )));
                    }
                };
                let expr = Expr::col(Alias::new(column));
                query.expr_as(
                    if kind == D1Type::I64 {
                        expr.cast_as(Alias::new("TEXT"))
                    } else {
                        expr
                    },
                    Alias::new(column),
                );
                projection = projection.column(column.clone(), kind, true)?;
            }
        } else {
            query.expr(Expr::cust("*"));
        }
        query.from(Alias::new(&name));
        for column in order {
            query.order_by((Alias::new(&name), Alias::new(*column)), Order::Asc);
        }
        if let Some((column, id)) = after {
            let value = if self.connection.requires_query_projection() {
                Expr::val(id.to_string()).cast_as(Alias::new("INTEGER"))
            } else {
                Expr::val(id)
            };
            query.and_where(Expr::col((Alias::new(&name), Alias::new(column))).gt(value));
        }
        if let Some(limit) = limit {
            query.limit(limit);
        }
        Ok(self
            .connection
            .query_rows(BatchQuery::new(backend.build(&query), projection))
            .await?)
    }
}
