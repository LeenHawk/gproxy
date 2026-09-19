//! libSQL / Turso over the Hrana HTTP pipeline (`POST {base}/v2/pipeline`).
//!
//! The connection speaks the JSON protocol; sending it is the caller's HTTP
//! transport (`LibsqlTransport`), so the same connection works natively and
//! on wasm32 with whatever fetch the host has. Writes are one pipeline
//! batch bracketed by `BEGIN`/`COMMIT` with a `ROLLBACK` step conditioned
//! on failure, so a failing statement rolls the whole batch back on the
//! server. No interactive transactions: `TransactionTrait` is not implemented.

use crate::{
    BatchConnectionTrait, BatchResult, BatchStatement, Projection, SelectProjection, codec, error,
    schema::ProjectedConnection,
};
use base64::Engine;
use sea_orm::{ConnectionTrait, DbBackend, DbErr, EntityTrait, ExecResult, QueryResult, Statement};
use serde_json::{Value as Json, json};
use std::{future::Future, pin::Pin, sync::Arc};

#[cfg(not(target_arch = "wasm32"))]
pub type LibsqlFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type LibsqlFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub trait TransportBounds: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> TransportBounds for T {}
#[cfg(target_arch = "wasm32")]
pub trait TransportBounds {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> TransportBounds for T {}

/// One pipeline call: an absolute URL, headers (authorization and content
/// type included) and a JSON body.
#[derive(Debug, Clone)]
pub struct LibsqlRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct LibsqlResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The HTTP leg, supplied by the host: a reqwest client natively, fetch on
/// wasm32, or anything that can POST JSON and return the whole body.
pub trait LibsqlTransport: TransportBounds {
    fn post<'a>(
        &'a self,
        request: LibsqlRequest,
    ) -> LibsqlFuture<'a, Result<LibsqlResponse, DbErr>>;
}

#[cfg(not(target_arch = "wasm32"))]
type Transport = Arc<dyn LibsqlTransport>;
#[cfg(target_arch = "wasm32")]
type Transport = send_wrapper::SendWrapper<Arc<dyn LibsqlTransport>>;

/// SeaORM connection to a libSQL server. Cloning shares the transport and
/// credentials; projections are per view like `D1Connection`.
#[derive(Clone)]
pub struct LibsqlConnection {
    transport: Transport,
    pipeline_url: Arc<str>,
    auth: Option<Arc<str>>,
    pub(crate) projection: Projection,
}

impl std::fmt::Debug for LibsqlConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibsqlConnection")
            .field("pipeline_url", &self.pipeline_url)
            .field("auth", &self.auth.as_ref().map(|_| "[redacted]"))
            .finish_non_exhaustive()
    }
}

/// SQLite's default variable limit; Turso keeps it.
pub const LIBSQL_MAX_BIND_PARAMETERS: usize = 32_766;

impl LibsqlConnection {
    /// `url` is the database URL (`libsql://`, `https://` or `http://`; the
    /// first is rewritten to https), `auth_token` the bearer token when the
    /// server requires one. Starts with a write-only projection.
    pub fn new(
        transport: Arc<dyn LibsqlTransport>,
        url: &str,
        auth_token: Option<&str>,
    ) -> Result<Self, DbErr> {
        let base = url.trim().trim_end_matches('/');
        let base = match base.split_once("://") {
            Some(("libsql", rest)) => format!("https://{rest}"),
            Some(("https" | "http", _)) => base.to_owned(),
            _ => return Err(error("libsql URL must be libsql://, https:// or http://")),
        };
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            transport,
            #[cfg(target_arch = "wasm32")]
            transport: send_wrapper::SendWrapper::new(transport),
            pipeline_url: format!("{base}/v2/pipeline").into(),
            auth: auth_token
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(Into::into),
            projection: Projection::new(),
        })
    }

    pub fn for_entity<E: EntityTrait>(&self) -> Result<Self, DbErr> {
        Ok(self.with_projection(Projection::for_entity::<E>()?))
    }

    pub fn for_select<Q: SelectProjection>(&self, query: &Q) -> Result<Self, DbErr> {
        Ok(self.with_projection(query.projection()?))
    }

    pub fn with_projection(&self, projection: Projection) -> Self {
        Self {
            transport: self.transport.clone(),
            pipeline_url: self.pipeline_url.clone(),
            auth: self.auth.clone(),
            projection,
        }
    }

    fn request(&self, requests: Vec<Json>) -> Result<LibsqlRequest, DbErr> {
        let mut headers = vec![("content-type".to_owned(), "application/json".to_owned())];
        if let Some(token) = &self.auth {
            headers.push(("authorization".to_owned(), format!("Bearer {token}")));
        }
        let body = serde_json::to_vec(&json!({ "baton": null, "requests": requests }))
            .map_err(|e| error(format!("encode Hrana pipeline: {e}")))?;
        Ok(LibsqlRequest {
            url: self.pipeline_url.to_string(),
            headers,
            body,
        })
    }

    /// One pipeline round trip: returns the `results` array.
    async fn pipeline(&self, requests: Vec<Json>) -> Result<Vec<Json>, DbErr> {
        let request = self.request(requests)?;
        let response = {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.transport.post(request).await?
            }
            #[cfg(target_arch = "wasm32")]
            {
                send_wrapper::SendWrapper::new(self.transport.post(request)).await?
            }
        };
        let body: Json = serde_json::from_slice(&response.body).map_err(|_| {
            error(format!(
                "libsql pipeline returned HTTP {} with a non-JSON body",
                response.status
            ))
        })?;
        if !(200..300).contains(&response.status) {
            let message = body
                .get("error")
                .and_then(|e| {
                    e.as_str()
                        .or_else(|| e.get("message").and_then(Json::as_str))
                })
                .unwrap_or("request rejected");
            return Err(error(format!(
                "libsql pipeline HTTP {}: {message}",
                response.status
            )));
        }
        body.get("results")
            .and_then(Json::as_array)
            .cloned()
            .ok_or_else(|| error("libsql pipeline response has no results"))
    }

    fn stmt(statement: &Statement, want_rows: bool) -> Result<Json, DbErr> {
        if statement.db_backend != DbBackend::Sqlite {
            return Err(error("libsql requires a SQLite statement"));
        }
        let mut args = Vec::new();
        if let Some(values) = &statement.values {
            if values.0.len() > LIBSQL_MAX_BIND_PARAMETERS {
                return Err(error("statement exceeds libsql's bound parameter limit"));
            }
            for value in &values.0 {
                args.push(hrana_value(codec::parameter(value)?));
            }
        }
        Ok(json!({
            "sql": statement.sql,
            "args": args,
            "want_rows": want_rows,
        }))
    }

    async fn execute_one(&self, statement: &Statement, want_rows: bool) -> Result<Json, DbErr> {
        let stmt = Self::stmt(statement, want_rows)?;
        let mut results = self
            .pipeline(vec![
                json!({"type": "execute", "stmt": stmt}),
                json!({"type": "close"}),
            ])
            .await?;
        if results.is_empty() {
            return Err(error("libsql pipeline returned no result"));
        }
        let first = results.remove(0);
        stream_ok(&first)?
            .get("result")
            .cloned()
            .ok_or_else(|| error("libsql execute response has no result"))
    }
}

impl ProjectedConnection for LibsqlConnection {
    fn with_projection(&self, projection: Projection) -> Self {
        LibsqlConnection::with_projection(self, projection)
    }
}

fn hrana_value(parameter: codec::Parameter) -> Json {
    match parameter {
        codec::Parameter::Null => json!({"type": "null"}),
        codec::Parameter::Number(n) if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 => {
            json!({"type": "integer", "value": (n as i64).to_string()})
        }
        codec::Parameter::Number(n) => json!({"type": "float", "value": n}),
        codec::Parameter::Text(text) => json!({"type": "text", "value": text}),
        codec::Parameter::Bytes(bytes) => json!({
            "type": "blob",
            "base64": base64::engine::general_purpose::STANDARD_NO_PAD.encode(bytes),
        }),
    }
}

/// An `ok` stream response's inner `response`; an `error` becomes a DbErr.
fn stream_ok(entry: &Json) -> Result<&Json, DbErr> {
    match entry.get("type").and_then(Json::as_str) {
        Some("ok") => entry
            .get("response")
            .ok_or_else(|| error("libsql ok response without payload")),
        Some("error") => Err(error(format!(
            "libsql: {}",
            entry
                .pointer("/error/message")
                .and_then(Json::as_str)
                .unwrap_or("unknown error")
        ))),
        _ => Err(error("libsql pipeline response of unknown type")),
    }
}

/// A Hrana cell as the JSON shape the shared codec decodes: exact integers as
/// text when they exceed the safe range, blobs as byte arrays.
fn cell(value: &Json) -> Result<Json, DbErr> {
    Ok(match value.get("type").and_then(Json::as_str) {
        Some("null") => Json::Null,
        Some("integer") => {
            let text = value
                .get("value")
                .and_then(Json::as_str)
                .ok_or_else(|| error("libsql integer without value"))?;
            let number: i64 = text
                .parse()
                .map_err(|_| error("libsql integer is not an i64"))?;
            if number.unsigned_abs() <= 9_007_199_254_740_991 {
                json!(number)
            } else {
                Json::String(text.to_owned())
            }
        }
        Some("float") => value
            .get("value")
            .cloned()
            .filter(Json::is_number)
            .ok_or_else(|| error("libsql float without value"))?,
        Some("text") => value
            .get("value")
            .cloned()
            .filter(Json::is_string)
            .ok_or_else(|| error("libsql text without value"))?,
        Some("blob") => {
            let encoded = value
                .get("base64")
                .and_then(Json::as_str)
                .ok_or_else(|| error("libsql blob without base64"))?;
            let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(encoded.trim_end_matches('='))
                .map_err(|_| error("libsql blob is not base64"))?;
            Json::Array(bytes.into_iter().map(|b| json!(b)).collect())
        }
        _ => return Err(error("libsql value of unknown type")),
    })
}

/// A `StmtResult` as D1's raw row array (`[[names], [cells]...]`).
fn raw_rows(result: &Json) -> Result<Json, DbErr> {
    let names: Vec<Json> = result
        .get("cols")
        .and_then(Json::as_array)
        .ok_or_else(|| error("libsql result has no cols"))?
        .iter()
        .map(|col| {
            col.get("name")
                .cloned()
                .filter(Json::is_string)
                .ok_or_else(|| error("libsql column without a name; alias every expression"))
        })
        .collect::<Result<_, _>>()?;
    let mut rows = vec![Json::Array(names)];
    for row in result
        .get("rows")
        .and_then(Json::as_array)
        .ok_or_else(|| error("libsql result has no rows"))?
    {
        let cells = row
            .as_array()
            .ok_or_else(|| error("libsql row is not an array"))?
            .iter()
            .map(cell)
            .collect::<Result<Vec<_>, _>>()?;
        rows.push(Json::Array(cells));
    }
    Ok(Json::Array(rows))
}

fn execution(result: &Json) -> Result<codec::DecodedExecution, DbErr> {
    let affected = result
        .get("affected_row_count")
        .and_then(Json::as_u64)
        .ok_or_else(|| error("libsql result has no affected_row_count"))?;
    let last = match result.get("last_insert_rowid") {
        Some(Json::String(text)) => text
            .parse::<i64>()
            .map_err(|_| error("libsql last_insert_rowid is not an integer"))?,
        Some(Json::Number(n)) => n.as_i64().unwrap_or(0),
        _ => 0,
    };
    Ok(codec::DecodedExecution {
        rows_affected: affected,
        last_insert_id: u64::try_from(last).unwrap_or(0),
    })
}

#[async_trait::async_trait]
impl ConnectionTrait for LibsqlConnection {
    fn get_database_backend(&self) -> DbBackend {
        DbBackend::Sqlite
    }

    fn support_returning(&self) -> bool {
        true
    }

    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        let result = self.execute_one(&statement, false).await?;
        execution(&result).map(Into::into)
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.execute_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await
    }

    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        Ok(self.query_all_raw(statement).await?.into_iter().next())
    }

    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        let result = self.execute_one(&statement, true).await?;
        Ok(codec::rows(&raw_rows(&result)?, &self.projection)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

#[async_trait::async_trait]
impl BatchConnectionTrait for LibsqlConnection {
    fn max_bind_parameters(&self) -> Option<usize> {
        Some(LIBSQL_MAX_BIND_PARAMETERS)
    }

    /// `BEGIN`, the steps each conditioned on the previous one succeeding,
    /// `COMMIT` conditioned on the last, `ROLLBACK` conditioned on its
    /// failure: the server never leaves a partial batch behind.
    async fn batch(&self, statements: &[BatchStatement]) -> Result<Vec<BatchResult>, DbErr> {
        if statements.is_empty() {
            return Ok(Vec::new());
        }
        let n = statements.len();
        let mut steps = Vec::with_capacity(n + 3);
        steps.push(json!({"stmt": {"sql": "BEGIN", "args": [], "want_rows": false}}));
        for (index, step) in statements.iter().enumerate() {
            step.validate(DbBackend::Sqlite)?;
            let want_rows = matches!(step, BatchStatement::Query(_));
            steps.push(json!({
                "condition": {"type": "ok", "step": index},
                "stmt": Self::stmt(step.statement(), want_rows)?,
            }));
        }
        steps.push(json!({
            "condition": {"type": "ok", "step": n},
            "stmt": {"sql": "COMMIT", "args": [], "want_rows": false},
        }));
        steps.push(json!({
            "condition": {"type": "not", "cond": {"type": "ok", "step": n}},
            "stmt": {"sql": "ROLLBACK", "args": [], "want_rows": false},
        }));
        let mut results = self
            .pipeline(vec![
                json!({"type": "batch", "batch": {"steps": steps}}),
                json!({"type": "close"}),
            ])
            .await?;
        if results.is_empty() {
            return Err(error("libsql batch returned no result"));
        }
        let first = results.remove(0);
        let batch = stream_ok(&first)?
            .get("result")
            .ok_or_else(|| error("libsql batch response has no result"))?;
        let step_results = batch
            .get("step_results")
            .and_then(Json::as_array)
            .ok_or_else(|| error("libsql batch has no step_results"))?;
        let step_errors = batch
            .get("step_errors")
            .and_then(Json::as_array)
            .ok_or_else(|| error("libsql batch has no step_errors"))?;
        if step_results.len() != n + 3 || step_errors.len() != n + 3 {
            return Err(error("libsql batch step count mismatch after dispatch"));
        }
        for (index, err) in step_errors.iter().enumerate().take(n + 2) {
            if !err.is_null() {
                let message = err
                    .get("message")
                    .and_then(Json::as_str)
                    .unwrap_or("statement failed");
                let what = match index {
                    0 => "BEGIN".to_owned(),
                    i if i <= n => format!("statement {}", i - 1),
                    _ => "COMMIT".to_owned(),
                };
                return Err(error(format!("libsql batch {what} failed: {message}")));
            }
        }
        if step_results[n + 1].is_null() {
            return Err(error("libsql batch did not commit"));
        }
        statements
            .iter()
            .zip(step_results.iter().skip(1))
            .map(|(step, result)| {
                if result.is_null() {
                    return Err(error("libsql batch step was skipped"));
                }
                match step {
                    BatchStatement::Execute(_) => {
                        execution(result).map(|r| BatchResult::Executed(r.into()))
                    }
                    BatchStatement::Query(query) => {
                        codec::rows(&raw_rows(result)?, &query.projection).map(|rows| {
                            BatchResult::Rows(rows.into_iter().map(Into::into).collect())
                        })
                    }
                }
            })
            .collect()
    }
}
