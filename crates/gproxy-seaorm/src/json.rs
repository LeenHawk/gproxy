use sea_orm::{DbBackend, DbErr, sea_query::Expr};

/// Exact, case-sensitive membership of a string in a top-level JSON array.
/// NULL, non-array documents and non-string elements never match. Arguments
/// remain SeaQuery expressions/bind parameters; no caller text becomes SQL.
/// SQLite includes D1. Other supported dialects are PostgreSQL and MySQL 8+.
/// Unimplemented backends return an error before query dispatch.
pub fn json_array_contains_text(
    backend: DbBackend,
    array: Expr,
    value: Expr,
) -> Result<Expr, DbErr> {
    Ok(match backend {
        DbBackend::Sqlite => Expr::cust_with_exprs(
            "(CASE WHEN json_type(?) = 'array' THEN EXISTS (\
                SELECT 1 FROM json_each(?) AS __gproxy_json_item \
                WHERE __gproxy_json_item.type = 'text' AND __gproxy_json_item.value COLLATE BINARY = ?) ELSE FALSE END)",
            [array.clone(), array, value],
        ),
        DbBackend::Postgres => Expr::cust_with_exprs(
            "EXISTS (SELECT 1 FROM jsonb_array_elements(CASE WHEN jsonb_typeof(CAST($1 AS jsonb)) = 'array' THEN CAST($1 AS jsonb) ELSE '[]'::jsonb END) AS __gproxy_json_item(value) WHERE jsonb_typeof(__gproxy_json_item.value) = 'string' AND (__gproxy_json_item.value #>> '{}') COLLATE \"C\" = CAST($2 AS text) COLLATE \"C\")",
            [array, value],
        ),
        DbBackend::MySql => Expr::cust_with_exprs(
            "EXISTS (SELECT 1 FROM JSON_TABLE(CASE WHEN JSON_TYPE(?) = 'ARRAY' THEN ? ELSE JSON_ARRAY() END, '$[*]' COLUMNS (ordinal FOR ORDINALITY)) AS __gproxy_json_item WHERE JSON_TYPE(JSON_EXTRACT(?, CONCAT('$[', __gproxy_json_item.ordinal - 1, ']'))) = 'STRING' AND CAST(JSON_UNQUOTE(JSON_EXTRACT(?, CONCAT('$[', __gproxy_json_item.ordinal - 1, ']'))) AS BINARY) = CAST(? AS BINARY))",
            [array.clone(), array.clone(), array.clone(), array, value],
        ),
        _ => {
            return Err(DbErr::Custom(format!(
                "JSON string-array membership is unsupported for {backend:?}"
            )));
        }
    })
}
