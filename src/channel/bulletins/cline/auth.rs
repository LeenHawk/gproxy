//! Cline auth: the credential Cline's own client presents, and the reader that
//! rebuilds it from a stored secret.
//!
//! Two credential shapes reach the same `Authorization: Bearer` header. A Cline
//! account access token is a WorkOS-issued JWT, and Cline's client prefixes it
//! with `workos:` before sending — the API distinguishes the two token families
//! by that prefix. A workspace API key is sent verbatim. Storing the token
//! unprefixed (as Cline does) keeps the JWT decodable for expiry checks.

use bytes::Bytes;
use http::Request;
use http::header::{HeaderName, HeaderValue};
use serde_json::Value;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

const WORKOS_PREFIX: &str = "workos:";

/// Client attribution Cline sends on every inference request; the gateway is
/// OpenRouter-backed and uses these for upstream routing attribution.
const CLIENT_HEADERS: &[(&str, &str)] = &[
    ("http-referer", "https://cline.bot"),
    ("x-title", "Cline"),
    ("x-client-type", "cline-sdk"),
];

/// Read a non-empty string field from the secret.
pub(super) fn field<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The bearer value for this credential: an account token carries the `workos:`
/// prefix, an API key does not.
pub(super) fn bearer(secret: &Value) -> Result<String, ChannelError> {
    if let Some(token) = field(secret, "access_token") {
        return Ok(if token.to_ascii_lowercase().starts_with(WORKOS_PREFIX) {
            token.to_string()
        } else {
            format!("{WORKOS_PREFIX}{token}")
        });
    }
    field(secret, "api_key")
        .map(str::to_string)
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token or api_key".into()))
}

/// Inject the credential plus Cline's client attribution headers.
pub(super) fn apply(req: &mut Request<Bytes>, secret: &Value) -> Result<(), ChannelError> {
    common::inject_bearer(req, &bearer(secret)?)?;
    for (name, value) in CLIENT_HEADERS {
        req.headers_mut().insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    Ok(())
}

/// Unix-seconds `exp` from a JWT access token. Cline's own client reads expiry
/// the same way when the stored `expiresAt` is absent, and it avoids an
/// RFC3339 parser that is native-only in this tree.
pub(super) fn token_expiry_secs(token: &str) -> Option<i64> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp").and_then(Value::as_i64)
}
