//! Shared secret redaction and size limits for captured HTTP wire data.

use http::HeaderMap;
use serde_json::{Map, Value};

use crate::app::snapshot::LogSettings;

const MAX_BODY: usize = 32 * 1024 * 1024;
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-goog-api-key",
    "cookie",
    "set-cookie",
];
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
    // Normalized camelCase OAuth/device-registration response keys.
    "clientsecret",
    "refreshtoken",
    "accesstoken",
    "idtoken",
    "devicecode",
];
const SECRET_PARAMS: &[&str] = &[
    "key",
    "api_key",
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "client_secret",
    "code",
    "assertion",
    "code_verifier",
    "client_assertion",
    "device_code",
    "subject_token",
    "sig",
    "signature",
    "jwt",
    "x-amz-credential",
    "x-amz-signature",
    "x-goog-credential",
    "x-goog-signature",
];
const REDACTED: &str = "[REDACTED]";

/// §14.3: redaction is forced ON unless explicitly disabled. Returns "redact?".
pub(crate) fn warn_unless_redacted(ls: &LogSettings) -> bool {
    warn_if_redaction_disabled(ls.disable_log_redaction)
}

pub(crate) fn warn_if_redaction_disabled(disabled: bool) -> bool {
    if disabled {
        tracing::warn!(
            "log redaction DISABLED (instance_settings.disable_log_redaction) — \
             captured request logs may contain credentials and PII"
        );
    }
    !disabled
}

pub(crate) fn headers_json(headers: &HeaderMap, redact: bool) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        let value = if redact && SECRET_HEADERS.contains(&name.as_str()) {
            REDACTED.to_owned()
        } else {
            String::from_utf8_lossy(value.as_bytes()).into_owned()
        };
        map.insert(name.as_str().to_owned(), Value::String(value));
    }
    Value::Object(map)
}

pub(crate) fn redact_query(query: &str, redact: bool) -> String {
    if !redact {
        return query.to_owned();
    }
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if is_secret_key(key) => format!("{key}={REDACTED}"),
            _ => pair.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Captured body with JSON and form-encoded secrets redacted, then size-capped.
pub(crate) fn body_string(body: &[u8], redact: bool) -> String {
    let mut body = String::from_utf8_lossy(body).into_owned();
    if redact {
        if let Ok(mut json) = serde_json::from_slice::<Value>(body.as_bytes()) {
            redact_json(&mut json);
            body = json.to_string();
        } else if let Some(form) = redact_form(&body) {
            body = form;
        }
    }
    if body.len() <= MAX_BODY {
        return body;
    }
    let mut cut = MAX_BODY;
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…[truncated {} bytes]", &body[..cut], body.len() - cut)
}

fn redact_form(body: &str) -> Option<String> {
    let pairs = body.split('&').collect::<Vec<_>>();
    if pairs.is_empty()
        || pairs
            .iter()
            .any(|pair| pair.split_once('=').is_none_or(|(key, _)| key.is_empty()))
    {
        return None;
    }
    Some(
        pairs
            .into_iter()
            .map(|pair| {
                let (key, value) = pair.split_once('=').expect("form pair checked above");
                if is_secret_key(key) {
                    format!("{key}={REDACTED}")
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
            .join("&"),
    )
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SECRET_FIELDS.contains(&key.as_str()) || SECRET_PARAMS.contains(&key.as_str())
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let secret_header_value = map
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| SECRET_HEADERS.contains(&name.to_ascii_lowercase().as_str()));
            if secret_header_value && let Some(value) = map.get_mut("value") {
                *value = Value::String(REDACTED.to_owned());
            }
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    *value = Value::String(REDACTED.to_owned());
                } else {
                    redact_json(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_json),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_headers_and_json_fields() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-123".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        let json = headers_json(&headers, true);
        assert_eq!(json["authorization"], REDACTED);
        assert_eq!(json["content-type"], "application/json");
        let body = br#"{"model":"m","api_key":"sk-1","nested":{"token":"t","ok":1},"clientSecret":"client-value","refreshToken":"refresh-value","accessToken":"access-value","idToken":"id-value","deviceCode":"device-value","code":"code-value"}"#;
        let out = body_string(body, true);
        for secret in [
            "sk-1",
            "\"t\"",
            "client-value",
            "refresh-value",
            "access-value",
            "id-value",
            "device-value",
            "code-value",
        ] {
            assert!(!out.contains(secret), "{out}");
        }
        assert!(out.contains("\"model\":\"m\""), "{out}");
        let custom = br#"{"customHeaders":[{"name":"X-API-Key","value":"sk-mcp"}]}"#;
        assert!(!body_string(custom, true).contains("sk-mcp"));
        assert_eq!(redact_query("alt=1&key=sk-9", true), "alt=1&key=[REDACTED]");
    }

    #[test]
    fn redacts_form_encoded_oauth_secrets() {
        let body = b"grant_type=refresh_token&refresh_token=rt&client_secret=cs&code=abc&code_verifier=cv&assertion=jwt&scope=openid";
        assert_eq!(
            body_string(body, true),
            "grant_type=refresh_token&refresh_token=[REDACTED]&client_secret=[REDACTED]&code=[REDACTED]&code_verifier=[REDACTED]&assertion=[REDACTED]&scope=openid"
        );
    }

    #[test]
    fn body_string_truncates_oversized() {
        let big = vec![b'a'; MAX_BODY + 4096];
        let out = body_string(&big, false);
        assert!(out.len() < big.len());
        assert!(out.contains("[truncated 4096 bytes]"));
    }
}
