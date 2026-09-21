//! Where a Vertex Express method lives. [`method_url`] is the single place
//! this channel computes an upstream URL.

use crate::channel::ChannelError;
use gproxy_protocol::{Dialect, Operation, OperationKey};

/// Express mode has no regional host: one global origin serves every key.
pub(super) const DEFAULT_ORIGIN: &str = "https://aiplatform.googleapis.com";

/// The complete upstream method URL for one request, key included.
///
/// An `endpoint_override` is the operator's own complete method URL and wins
/// outright; only the query is still assembled around it. Otherwise the method
/// is `{origin}/v1/publishers/google/models/{model}:{verb}`, where `origin` is
/// the provider's `base_url` when it has one and [`DEFAULT_ORIGIN`] otherwise,
/// and `verb` is `generateContent`, `streamGenerateContent` or `countTokens`.
///
/// The key is appended as `key={api_key}`, which is how express mode
/// authenticates. An override that already carries a `key` parameter is a
/// configuration mistake — the operator has pasted a credential into a URL —
/// and is refused rather than joined with the assigned one.
pub(super) fn method_url(
    base_url: Option<&str>,
    endpoint_override: Option<&str>,
    operation: OperationKey,
    path: &str,
    query: Option<&str>,
    api_key: &str,
) -> Result<String, ChannelError> {
    let (url, inherited) = match trimmed(endpoint_override) {
        Some(url) => {
            let (head, tail) = match url.split_once('?') {
                Some((head, tail)) => (head, Some(tail)),
                None => (url, None),
            };
            if tail.is_some_and(|tail| tail.split('&').any(|part| name_of(part) == "key")) {
                return Err(ChannelError::InvalidConfig(
                    "a Vertex Express endpoint override must not embed an API key".into(),
                ));
            }
            (head.to_owned(), tail)
        }
        None => {
            let origin = trimmed(base_url)
                .map(|base| base.trim_end_matches('/'))
                .unwrap_or(DEFAULT_ORIGIN);
            (format!("{origin}{}", method_path(operation, path)?), None)
        }
    };
    let mut parts: Vec<String> = [inherited, query]
        .into_iter()
        .flatten()
        .flat_map(|source| source.split('&'))
        .filter(|part| !part.is_empty() && !is_auth_parameter(part))
        .map(str::to_owned)
        .collect();
    parts.push(format!("key={}", percent_encode(api_key)));
    Ok(format!("{url}?{}", parts.join("&")))
}

fn method_path(operation: OperationKey, path: &str) -> Result<String, ChannelError> {
    let verb = match (operation.operation, operation.dialect) {
        (Operation::GenerateContent, Dialect::Gemini) => "generateContent",
        (Operation::StreamGenerateContent, Dialect::Gemini) => "streamGenerateContent",
        (Operation::CountTokens, Dialect::Gemini) => "countTokens",
        _ => return Err(ChannelError::UnsupportedOperation(operation)),
    };
    let model = model_from_path(path).ok_or_else(|| {
        ChannelError::InvalidConfig(
            "a Vertex Express request must name its model in the path".into(),
        )
    })?;
    validate_model(model)?;
    Ok(format!("/v1/publishers/google/models/{model}:{verb}"))
}

/// The bare model id a Gemini path names, in any of its spellings
/// (`{id}`, `models/{id}`, `publishers/google/models/{id}`), with the method
/// verb after `:` removed.
fn model_from_path(path: &str) -> Option<&str> {
    let tail = match path.rsplit_once("/models/") {
        Some((_, tail)) => tail,
        None => path.rsplit('/').next()?,
    };
    let id = tail.split(':').next().unwrap_or(tail);
    (!id.is_empty()).then_some(id)
}

/// A model id lands in a URL path segment; a value that cannot be one is
/// refused rather than percent-encoded into a request for a resource that
/// does not exist.
fn validate_model(model: &str) -> Result<(), ChannelError> {
    let valid = model
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'@'));
    if valid {
        Ok(())
    } else {
        Err(ChannelError::InvalidConfig(format!(
            "`{model}` is not a Gemini model id"
        )))
    }
}

/// Source authentication never reaches the upstream; the assigned key does.
fn is_auth_parameter(part: &str) -> bool {
    matches!(name_of(part), "key" | "access_token" | "api_key")
}

fn name_of(part: &str) -> &str {
    part.split('=').next().unwrap_or_default()
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}
