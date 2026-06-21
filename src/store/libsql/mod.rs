//! Minimal libSQL/Turso **Hrana-over-HTTP** client for wasm32 targets.
//!
//! POSTs to `{url}/v2/pipeline` with `Authorization: Bearer {token}`.
//! Native code uses SeaORM directly (`store/persistence/db.rs`).
//!
//! Hrana v2 pipeline request:
//! `{"requests":[{"type":"execute","stmt":{"sql":"...","args":[...]}},{"type":"close"}]}`
//!
//! Response: `{"baton":..,"results":[{"type":"ok","response":{"type":"execute","result":{...}}},
//! {"type":"ok","response":{"type":"close"}}]}` — or `{"type":"error","error":{"message":"..."}}`.

mod wire;

use js_sys::{Uint8Array, global};
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response, WorkerGlobalScope};

use serde_json::json;

pub use wire::{Col, QueryResult};
use wire::{HranaOkResponse, HranaResponse, HranaResult, Pipeline, PipelineRequest, Stmt};

// ── Hrana typed-value args ──────────────────────────────────────────────────────
//
// Hrana v2 requires every bound statement arg to be an internally-tagged
// typed-value object, NOT a bare JSON scalar. Passing a bare value yields
// `invalid type: string "hi", expected internally tagged enum Value`.
// Note that the integer form carries its value as a *quoted string*.

/// Bind a text arg: `{"type":"text","value":"..."}`.
pub fn arg_text(s: &str) -> Value {
    json!({ "type": "text", "value": s })
}

/// Bind an integer arg: `{"type":"integer","value":"123"}` (value is a string).
pub fn arg_integer(n: i64) -> Value {
    json!({ "type": "integer", "value": n.to_string() })
}

/// Bind a blob arg: `{"type":"blob","base64":"..."}`.
pub fn arg_blob(bytes: &[u8]) -> Value {
    json!({ "type": "blob", "base64": crate::store::cache::b64::encode(bytes) })
}

/// Bind a null arg: `{"type":"null"}`.
pub fn arg_null() -> Value {
    json!({ "type": "null" })
}

/// Errors from the libSQL HTTP client.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("fetch error: {0}")]
    Fetch(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Hrana error: {0}")]
    Hrana(String),
    #[error("unexpected response shape")]
    BadResponse,
}

fn js_err(e: wasm_bindgen::JsValue) -> StoreError {
    StoreError::Fetch(format!("{e:?}"))
}

// ── Client ────────────────────────────────────────────────────────────────────

/// Minimal Hrana-over-HTTP client for Turso/libSQL.
pub struct LibsqlClient {
    url: String,
    token: String,
}

impl LibsqlClient {
    /// Create a new client. `url` is the Turso database URL
    /// (e.g. `https://<db>.turso.io`); `token` is the auth token.
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }

    /// Execute a single SQL statement via Hrana v2 pipeline.
    pub async fn execute(&self, sql: &str, args: &[Value]) -> Result<QueryResult, StoreError> {
        let pipeline_url = format!("{}/v2/pipeline", self.url);

        let body = serde_json::to_string(&Pipeline {
            requests: vec![
                PipelineRequest::Execute {
                    stmt: Stmt { sql, args },
                },
                PipelineRequest::Close,
            ],
        })?;

        // Build fetch request.
        let js_headers = Headers::new().map_err(js_err)?;
        js_headers
            .append("Content-Type", "application/json")
            .map_err(js_err)?;
        js_headers
            .append("Authorization", &format!("Bearer {}", self.token))
            .map_err(js_err)?;

        let body_arr = {
            let bytes = body.as_bytes();
            let arr = Uint8Array::new_with_length(bytes.len() as u32);
            arr.copy_from(bytes);
            arr
        };

        let init = RequestInit::new();
        init.set_method("POST");
        init.set_headers_headers(&js_headers);
        init.set_body_opt_u8_array(Some(&body_arr));

        let js_req = Request::new_with_str_and_init(&pipeline_url, &init).map_err(js_err)?;

        let scope = global().unchecked_into::<WorkerGlobalScope>();
        let resp_val = JsFuture::from(scope.fetch_with_request(&js_req))
            .await
            .map_err(js_err)?;
        let js_resp: Response = resp_val.unchecked_into();

        let buf_promise = js_resp.array_buffer().map_err(js_err)?;
        let buf_val = JsFuture::from(buf_promise).await.map_err(js_err)?;
        let body_bytes = Uint8Array::new(&buf_val).to_vec();

        // Parse into a generic Value first so we can detect a top-level Hrana
        // error envelope — e.g. auth failures, bad URL, or quota exceeded return
        // `{"type":"error","error":{"message":"..."}}` without a `results` field,
        // which would otherwise produce a misleading "missing field `results`" error.
        let val: serde_json::Value = serde_json::from_slice(&body_bytes)?;
        if val.get("type").and_then(|t| t.as_str()) == Some("error") {
            let msg = val
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or_else(|| val.as_str().unwrap_or("unknown Hrana top-level error"))
                .to_owned();
            return Err(StoreError::Hrana(msg));
        }
        if val.get("results").is_none() {
            let msg = val.to_string();
            return Err(StoreError::Hrana(msg));
        }
        let hrana: HranaResponse = serde_json::from_value(val)?;

        // First result is the execute; second is close. Extract execute result.
        let mut iter = hrana.results.into_iter();
        match iter.next().ok_or(StoreError::BadResponse)? {
            HranaResult::Ok {
                response: HranaOkResponse::Execute { result },
            } => Ok(QueryResult {
                cols: result.cols,
                rows: result.rows,
                affected_row_count: result.affected_row_count,
                last_insert_rowid: result.last_insert_rowid,
            }),
            HranaResult::Error { error } => Err(StoreError::Hrana(error.message)),
            HranaResult::Ok {
                response: HranaOkResponse::Close,
            } => Err(StoreError::BadResponse),
        }
    }
}
