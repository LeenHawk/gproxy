//! Secret redaction and size limits for captured wire data.

use http::HeaderMap;
use serde_json::{Map, Value};

use crate::app::snapshot::LogSettings;

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

/// §14.3: redaction is forced ON unless `disable_log_redaction` — and then
/// every captured entry prints a loud warning. Returns "redact?".
pub(super) fn warn_unless_redacted(ls: &LogSettings) -> bool {
    if ls.disable_log_redaction {
        tracing::warn!(
            "log redaction DISABLED (instance_settings.disable_log_redaction) — \
             captured request logs may contain credentials and PII"
        );
    }
    !ls.disable_log_redaction
}

/// Header map → JSON object; secret headers replaced by `[REDACTED]`.
pub(super) fn headers_json(headers: &HeaderMap, redact: bool) -> Value {
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
pub(super) fn redact_query(query: &str, redact: bool) -> String {
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
pub(super) fn body_string(body: &[u8], redact: bool) -> String {
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
