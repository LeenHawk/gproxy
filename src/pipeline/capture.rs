//! §8-D request capture: `downstream_requests` / `upstream_requests` wire logs
//! gated by the instance log toggles (§8-E), with §14.3 secret redaction.
//! Native writes are fire-and-forget spawns; wasm awaits inline (no detached
//! tasks on edge).

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::{Map, Value};

use crate::app::AppState;
use crate::app::snapshot::LogSettings;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::store::persistence::records::{DownstreamRequestInput, UpstreamRequestInput};
use crate::util::time::unix_now;

/// Body capture cap — bodies larger than this are truncated in the log row.
const MAX_BODY: usize = 32 * 1024 * 1024;

/// Headers whose values are secrets (§14.3): always stripped from captured
/// logs unless redaction is explicitly disabled.
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-goog-api-key",
    "cookie",
    "set-cookie",
];

/// JSON body fields treated as secrets (§14.3 "known secret fields").
const SECRET_FIELDS: &[&str] = &[
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "client_secret",
    "secret",
    "password",
    "authorization",
];

/// Query params whose values are secrets (Gemini `?key=`).
const SECRET_PARAMS: &[&str] = &["key", "api_key", "token", "access_token"];

const REDACTED: &str = "[REDACTED]";

/// The downstream wire facts, captured BEFORE the pipeline mutates the request
/// (the ingress blacklist strips client creds in place); written after the
/// response status is known. `None` = downstream capture disabled.
pub struct DownstreamCapture {
    at: i64,
    request_id: String,
    method: String,
    path: String,
    query: Option<String>,
    headers_json: Option<Value>,
    body: Option<String>,
}

/// Capture the inbound request if `enable_downstream_log` is on.
pub fn downstream_precapture(state: &AppState, ctx: &RequestCtx) -> Option<DownstreamCapture> {
    let ls = state.cp().log_settings.clone();
    if !ls.enable_downstream_log {
        return None;
    }
    let redact = warn_unless_redacted(&ls);
    Some(DownstreamCapture {
        at: unix_now(),
        request_id: ctx.request_id.clone(),
        method: ctx.method.to_string(),
        path: ctx.path.clone(),
        query: ctx.query.as_deref().map(|q| redact_query(q, redact)),
        headers_json: Some(headers_json(&ctx.headers, redact)),
        body: ls
            .enable_downstream_log_body
            .then(|| body_string(&ctx.body, redact)),
    })
}

/// Append the captured downstream request with its final `status`. For
/// non-streaming responses the body is folded into the same INSERT via
/// `response_body` (streaming responses pass `None` and backfill later via
/// [`record_downstream_response`]). Gated by the downstream log-body toggle.
pub async fn log_downstream(
    state: &AppState,
    cap: DownstreamCapture,
    status: StatusCode,
    response_body: Option<&[u8]>,
) {
    let resp = response_body.map(|b| {
        let ls = state.cp().log_settings.clone();
        let redact = warn_unless_redacted(&ls);
        body_string(b, redact)
    });
    let input = DownstreamRequestInput {
        request_id: cap.request_id,
        at: cap.at,
        method: cap.method,
        path: cap.path,
        query: cap.query,
        status: i64::from(status.as_u16()),
        headers_json: cap.headers_json,
        body: cap.body,
        response_body: resp,
    };
    persist(state, Row::Downstream(input)).await;
}

/// The final attempt's wire facts handed to [`log_upstream`].
pub struct UpstreamWire<'a> {
    pub status: StatusCode,
    pub latency_ms: i64,
    pub url: &'a str,
    pub method: &'a http::Method,
    /// Prepared request headers — captured by the attempt only when the
    /// upstream-log toggle was on.
    pub sent_headers: Option<&'a HeaderMap>,
    pub sent_body: &'a Bytes,
    /// Non-streaming upstream (provider) response body, post channel-decode /
    /// pre transform; `None` for streams (the spliced guard backfills those).
    pub resp_body: Option<&'a Bytes>,
}

/// Append the final (returned-to-client) upstream attempt's wire facts if
/// `enable_upstream_log` is on.
pub async fn log_upstream(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    w: UpstreamWire<'_>,
) {
    let ls = state.cp().log_settings.clone();
    if !ls.enable_upstream_log {
        return;
    }
    let redact = warn_unless_redacted(&ls);
    let input = UpstreamRequestInput {
        request_id: ctx.request_id.clone(),
        at: unix_now(),
        provider_id: Some(cand.provider.id),
        credential_id: Some(cand.credential.id),
        url: w.url.to_owned(),
        method: w.method.to_string(),
        status: i64::from(w.status.as_u16()),
        latency_ms: w.latency_ms,
        headers_json: w.sent_headers.map(|h| headers_json(h, redact)),
        body: ls
            .enable_upstream_log_body
            .then(|| body_string(w.sent_body, redact)),
        response_body: w
            .resp_body
            .filter(|_| ls.enable_upstream_log_body)
            .map(|b| body_string(b, redact)),
    };
    persist(state, Row::Upstream(input)).await;
}

enum Row {
    Downstream(DownstreamRequestInput),
    Upstream(UpstreamRequestInput),
}

async fn persist(state: &AppState, row: Row) {
    async fn write(db: &dyn crate::store::persistence::PersistenceBackend, row: Row) {
        let result = match row {
            Row::Downstream(input) => db.append_downstream_request(input).await.map(|_| ()),
            Row::Upstream(input) => db.append_upstream_request(input).await.map(|_| ()),
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "request-capture log write failed");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let persistence = std::sync::Arc::clone(&state.persistence);
        tokio::spawn(async move { write(persistence.as_ref(), row).await });
    }
    #[cfg(target_arch = "wasm32")]
    write(state.persistence.as_ref(), row).await;
}

/// Backfill the captured DOWNSTREAM response body for a streaming response (the
/// row was appended before the stream settled). Gated by the downstream
/// log-body toggle; redacted + capped by [`body_string`]. Fire-and-forget.
pub async fn record_downstream_response(state: &AppState, request_id: &str, body: &[u8]) {
    let ls = state.cp().log_settings.clone();
    if !(ls.enable_downstream_log && ls.enable_downstream_log_body) {
        return;
    }
    let redact = warn_unless_redacted(&ls);
    let s = body_string(body, redact);
    persist_response(state, RespRow::Downstream(request_id.to_owned(), s)).await;
}

/// Backfill the captured UPSTREAM response body for a streaming response.
pub async fn record_upstream_response(state: &AppState, request_id: &str, body: &[u8]) {
    let ls = state.cp().log_settings.clone();
    if !(ls.enable_upstream_log && ls.enable_upstream_log_body) {
        return;
    }
    let redact = warn_unless_redacted(&ls);
    let s = body_string(body, redact);
    persist_response(state, RespRow::Upstream(request_id.to_owned(), s)).await;
}

enum RespRow {
    Downstream(String, String),
    Upstream(String, String),
}

async fn persist_response(state: &AppState, row: RespRow) {
    async fn write(db: &dyn crate::store::persistence::PersistenceBackend, row: RespRow) {
        let result = match row {
            RespRow::Downstream(rid, body) => db.update_downstream_response(&rid, Some(body)).await,
            RespRow::Upstream(rid, body) => db.update_upstream_response(&rid, Some(body)).await,
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "response-capture log write failed");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let persistence = std::sync::Arc::clone(&state.persistence);
        tokio::spawn(async move { write(persistence.as_ref(), row).await });
    }
    #[cfg(target_arch = "wasm32")]
    write(state.persistence.as_ref(), row).await;
}

/// §14.3: redaction is forced ON unless `disable_log_redaction` — and then
/// every captured entry prints a loud warning. Returns "redact?".
fn warn_unless_redacted(ls: &LogSettings) -> bool {
    if ls.disable_log_redaction {
        tracing::warn!(
            "log redaction DISABLED (instance_settings.disable_log_redaction) — \
             captured request logs may contain credentials and PII"
        );
    }
    !ls.disable_log_redaction
}

/// Header map → JSON object; secret headers replaced by `[REDACTED]`.
fn headers_json(headers: &HeaderMap, redact: bool) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        let v = if redact && SECRET_HEADERS.contains(&name.as_str()) {
            REDACTED.to_owned()
        } else {
            String::from_utf8_lossy(value.as_bytes()).into_owned()
        };
        map.insert(name.as_str().to_owned(), Value::String(v));
    }
    Value::Object(map)
}

/// Query string with secret param values replaced (`key=…` → `key=[REDACTED]`).
fn redact_query(query: &str, redact: bool) -> String {
    if !redact {
        return query.to_owned();
    }
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if SECRET_PARAMS.contains(&k.to_ascii_lowercase().as_str()) => {
                format!("{k}={REDACTED}")
            }
            _ => pair.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Body → capped string; JSON bodies get known secret fields redacted in place.
fn body_string(body: &[u8], redact: bool) -> String {
    let s = if redact && let Ok(mut v) = serde_json::from_slice::<Value>(body) {
        redact_json(&mut v);
        v.to_string()
    } else {
        String::from_utf8_lossy(body).into_owned()
    };
    if s.len() > MAX_BODY {
        let mut cut = MAX_BODY;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}…[truncated {} bytes]", &s[..cut], s.len() - cut)
    } else {
        s
    }
}

/// Recursively replace known secret fields in a JSON value.
fn redact_json(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if SECRET_FIELDS.contains(&k.to_ascii_lowercase().as_str()) {
                    *val = Value::String(REDACTED.to_owned());
                } else {
                    redact_json(val);
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(redact_json),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_headers_and_json_fields() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer sk-123".parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        let j = headers_json(&h, true);
        assert_eq!(j["authorization"], REDACTED);
        assert_eq!(j["content-type"], "application/json");

        let body = br#"{"model":"m","api_key":"sk-1","nested":{"token":"t","ok":1}}"#;
        let out = body_string(body, true);
        assert!(!out.contains("sk-1") && !out.contains("\"t\""), "{out}");
        assert!(out.contains("\"model\":\"m\""), "{out}");

        assert_eq!(redact_query("alt=1&key=sk-9", true), "alt=1&key=[REDACTED]");
    }

    #[test]
    fn body_string_truncates_oversized() {
        // Response bodies reuse body_string; a payload well past MAX_BODY must
        // be capped with the truncation marker (not silently kept whole).
        let big = vec![b'a'; MAX_BODY + 4096];
        let out = body_string(&big, false);
        assert!(out.len() < big.len(), "oversized body should be truncated");
        assert!(out.contains("[truncated 4096 bytes]"), "missing marker");
    }
}
