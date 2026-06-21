//! Hrana v2 wire types: request serialization + response deserialization.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single column descriptor returned by Hrana.
#[derive(Debug, Deserialize)]
pub struct Col {
    pub name: Option<String>,
}

/// Result of a single `execute` statement.
#[derive(Debug)]
pub struct QueryResult {
    pub cols: Vec<Col>,
    /// Each row is a `Vec<Value>` of Hrana typed-value objects; callers can
    /// inspect `value["type"]` / `value["value"]` for typed extraction.
    pub rows: Vec<Vec<Value>>,
    pub affected_row_count: u64,
    pub last_insert_rowid: Option<String>,
}

// ── Response deserialization ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub(super) struct HranaResponse {
    pub results: Vec<HranaResult>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum HranaResult {
    Ok { response: HranaOkResponse },
    Error { error: HranaError },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum HranaOkResponse {
    Execute { result: HranaExecuteResult },
    Close,
}

#[derive(Deserialize)]
pub(super) struct HranaExecuteResult {
    pub cols: Vec<Col>,
    pub rows: Vec<Vec<Value>>,
    pub affected_row_count: u64,
    pub last_insert_rowid: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct HranaError {
    pub message: String,
}

// ── Request serialization ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub(super) struct Pipeline<'a> {
    pub requests: Vec<PipelineRequest<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum PipelineRequest<'a> {
    Execute { stmt: Stmt<'a> },
    Close,
}

#[derive(Serialize)]
pub(super) struct Stmt<'a> {
    pub sql: &'a str,
    pub args: &'a [Value],
}
