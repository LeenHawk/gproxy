use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::super::libsql::{HttpFuture, HttpSender};
use super::super::native::NativeSql;
use super::super::{DbValue, Executor, QueryResult, Statement};
use crate::StoreError;

pub(super) struct SqliteHrana {
    database: Arc<NativeSql>,
}

impl SqliteHrana {
    pub(super) fn new(database: Arc<NativeSql>) -> Self {
        Self { database }
    }
}

impl HttpSender for SqliteHrana {
    fn post<'a>(&'a self, url: &'a str, token: &'a str, body: Vec<u8>) -> HttpFuture<'a> {
        Box::pin(async move {
            if url != "https://store.invalid/v2/pipeline" || token != "test-token" {
                return Err(StoreError::Database("unexpected Hrana destination".into()));
            }
            let request: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|error| StoreError::Database(error.to_string()))?;
            let first = &request["requests"][0];
            let response = match first["type"].as_str() {
                Some("execute") => execute(self.database.as_ref(), &first["stmt"]).await?,
                Some("batch") => batch(self.database.as_ref(), &first["batch"]).await?,
                _ => return Err(StoreError::Database("unexpected Hrana request".into())),
            };
            serde_json::to_vec(&serde_json::json!({
                "baton": null,
                "results": [
                    {"type": "ok", "response": response},
                    {"type": "ok", "response": {"type": "close"}}
                ]
            }))
            .map_err(|error| StoreError::Database(error.to_string()))
        })
    }
}

async fn execute(
    database: &NativeSql,
    value: &serde_json::Value,
) -> Result<serde_json::Value, StoreError> {
    let result = database.execute(decode_statement(value)?).await?;
    Ok(serde_json::json!({"type": "execute", "result": encode_result(&result)}))
}

async fn batch(
    database: &NativeSql,
    value: &serde_json::Value,
) -> Result<serde_json::Value, StoreError> {
    let steps = value["steps"]
        .as_array()
        .ok_or_else(|| StoreError::Database("Hrana batch steps missing".into()))?;
    let statements = steps
        .iter()
        .map(|step| decode_statement(&step["stmt"]))
        .collect::<Result<Vec<_>, _>>()?;
    if statements.first().map(|statement| statement.sql.as_str()) != Some("BEGIN")
        || statements.last().map(|statement| statement.sql.as_str()) != Some("END")
    {
        return Err(StoreError::Database(
            "Hrana transaction envelope missing".into(),
        ));
    }
    let results = database
        .batch(statements[1..statements.len() - 1].to_vec())
        .await?;
    let mut encoded = vec![Some(encode_result(&QueryResult::default()))];
    encoded.extend(results.iter().map(|result| Some(encode_result(result))));
    encoded.push(Some(encode_result(&QueryResult::default())));
    Ok(serde_json::json!({
        "type": "batch",
        "result": {
            "step_errors": vec![Option::<serde_json::Value>::None; encoded.len()],
            "step_results": encoded
        }
    }))
}

fn decode_statement(value: &serde_json::Value) -> Result<Statement, StoreError> {
    let sql = value["sql"]
        .as_str()
        .ok_or_else(|| StoreError::Database("Hrana SQL missing".into()))?
        .to_owned();
    let args = value["args"]
        .as_array()
        .ok_or_else(|| StoreError::Database("Hrana args missing".into()))?
        .iter()
        .map(decode_value)
        .collect::<Result<_, _>>()?;
    Ok(Statement::with_args(sql, args))
}

fn decode_value(value: &serde_json::Value) -> Result<DbValue, StoreError> {
    Ok(match value["type"].as_str() {
        Some("null") => DbValue::Null,
        Some("integer") => DbValue::Integer(
            value["value"]
                .as_str()
                .ok_or_else(|| StoreError::Database("integer value missing".into()))?
                .parse()
                .map_err(|error| StoreError::Database(format!("invalid integer: {error}")))?,
        ),
        Some("float") => DbValue::Real(
            value["value"]
                .as_f64()
                .ok_or_else(|| StoreError::Database("float value missing".into()))?,
        ),
        Some("text") => DbValue::Text(
            value["value"]
                .as_str()
                .ok_or_else(|| StoreError::Database("text value missing".into()))?
                .to_owned(),
        ),
        Some("blob") => DbValue::Blob(
            BASE64
                .decode(value["base64"].as_str().unwrap_or_default())
                .map_err(|error| StoreError::Database(error.to_string()))?,
        ),
        _ => return Err(StoreError::Database("unknown Hrana value".into())),
    })
}

fn encode_result(result: &QueryResult) -> serde_json::Value {
    let columns = result
        .rows
        .first()
        .map(|row| row.entries().map(|(name, _)| name).collect::<Vec<_>>())
        .unwrap_or_default();
    let rows = result
        .rows
        .iter()
        .map(|row| {
            row.entries()
                .map(|(_, value)| encode_value(value))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "cols": columns.into_iter().map(|name| serde_json::json!({"name": name})).collect::<Vec<_>>(),
        "rows": rows,
        "affected_row_count": result.affected_rows,
        "last_insert_rowid": result.last_insert_id.map(|value| value.to_string())
    })
}

fn encode_value(value: &DbValue) -> serde_json::Value {
    match value {
        DbValue::Null => serde_json::json!({"type": "null"}),
        DbValue::Integer(value) => {
            serde_json::json!({"type": "integer", "value": value.to_string()})
        }
        DbValue::Real(value) => serde_json::json!({"type": "float", "value": value}),
        DbValue::Text(value) => serde_json::json!({"type": "text", "value": value}),
        DbValue::Blob(value) => serde_json::json!({"type": "blob", "base64": BASE64.encode(value)}),
    }
}
