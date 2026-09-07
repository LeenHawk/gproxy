use http::HeaderMap;
use serde_json::{Map, Value};

pub(super) const MAX_BODY_BYTES: usize = 100 * 1024 * 1024;
const REDACTED: &str = "[redacted]";
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-goog-api-key",
    "api-key",
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
    "fallback_credit_token",
    "id_token",
    "client_secret",
    "secret",
    "password",
    "authorization",
    "clientsecret",
    "refreshtoken",
    "accesstoken",
    "idtoken",
    "devicecode",
    "code",
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
    "state",
    "authorization_code",
    "device_auth_id",
    "user_code",
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

pub(crate) fn headers_json(headers: &HeaderMap, redact: bool) -> Value {
    let mut values = Map::new();
    for (name, value) in headers {
        let value = if redact && SECRET_HEADERS.contains(&name.as_str()) {
            REDACTED.to_owned()
        } else {
            String::from_utf8_lossy(value.as_bytes()).into_owned()
        };
        values.insert(name.as_str().to_owned(), Value::String(value));
    }
    Value::Object(values)
}

pub(super) fn query_string(query: &str, redact: bool) -> String {
    if !redact {
        return query.to_owned();
    }
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _)) if secret_key(key) => format!("{key}={REDACTED}"),
            _ => pair.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub(crate) fn url_string(url: &str, redact: bool) -> String {
    match url.split_once('?') {
        Some((path, query)) => format!("{path}?{}", query_string(query, redact)),
        None => url.to_owned(),
    }
}

pub(crate) fn body_bytes(body: &[u8], redact: bool) -> Vec<u8> {
    let mut body = String::from_utf8_lossy(body).into_owned();
    if redact {
        if let Ok(mut json) = serde_json::from_slice::<Value>(body.as_bytes()) {
            redact_json(&mut json);
            body = json.to_string();
        } else if let Some(form) = redact_form(&body) {
            body = form;
        }
    }
    if body.len() <= MAX_BODY_BYTES {
        return body.into_bytes();
    }
    let mut cut = MAX_BODY_BYTES;
    while !body.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…[truncated {} bytes]", &body[..cut], body.len() - cut).into_bytes()
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
                let (key, value) = pair.split_once('=').expect("form pair checked");
                if secret_key(key) {
                    format!("{key}={REDACTED}")
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
            .join("&"),
    )
}

fn secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    SECRET_FIELDS.contains(&key.as_str()) || SECRET_PARAMS.contains(&key.as_str())
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(values) => {
            let secret_header = values
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| SECRET_HEADERS.contains(&name.to_ascii_lowercase().as_str()));
            if secret_header && let Some(value) = values.get_mut("value") {
                *value = Value::String(REDACTED.to_owned());
            }
            for (key, value) in values {
                if secret_key(key) {
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
