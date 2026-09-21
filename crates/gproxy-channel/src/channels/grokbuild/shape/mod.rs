//! The Responses body the Grok Build proxy accepts.
//!
//! The proxy is not OpenAI: it rejects a continuation id it never issued,
//! several bookkeeping fields, a non-positive `top_p`, tool shapes it has no
//! counterpart for, and encrypted reasoning it did not produce. v3 narrowed
//! the body in `grokbuild/shape/responses/`; this is the same narrowing.
//!
//! One field is added rather than removed: a `grok-composer-*` model needs a
//! `prompt_cache_key`, which also becomes the `x-grok-conv-id` header, so one
//! is minted when the caller sent none.

mod reasoning;
mod tools;

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Map, Value};

/// Fields the proxy has no place for (v3 `shape/responses/mod.rs`).
const DROPPED: &[&str] = &[
    "previous_response_id",
    "metadata",
    "prompt_cache_retention",
    "safety_identifier",
    "stream_options",
];

/// Only this model family is given a cache key it did not ask for.
const COMPOSER_PREFIX: &str = "grok-composer-";

pub(super) fn request(body: &[u8]) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid(format!("the Responses body is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("the Responses body must be an object"))?;
    for name in DROPPED {
        object.remove(*name);
    }
    // The proxy reads a zero or negative `top_p` as "sample nothing".
    if object
        .get("top_p")
        .and_then(Value::as_f64)
        .is_some_and(|value| value <= 0.0)
    {
        object.remove("top_p");
    }
    tools::normalize(object);
    reasoning::sanitize(object);
    ensure_cache_key(object)?;
    Ok(Bytes::from(value.to_string()))
}

/// The conversation key the header mirrors, once the body is final.
pub(super) fn cache_key(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()?
        .get("prompt_cache_key")?
        .as_str()
        .map(str::to_owned)
}

fn ensure_cache_key(object: &mut Map<String, Value>) -> Result<(), ChannelError> {
    let stated = object
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    let composer = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .starts_with(COMPOSER_PREFIX);
    if stated || !composer {
        return Ok(());
    }
    object.insert("prompt_cache_key".into(), Value::String(uuid()?));
    Ok(())
}

/// A version-4 UUID, the shape the CLI's cache keys have.
fn uuid() -> Result<String, ChannelError> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = |slice: &[u8]| {
        slice.iter().fold(String::new(), |mut out, byte| {
            let _ = write!(&mut out, "{byte:02x}");
            out
        })
    };
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    ))
}

pub(super) fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(format!("Grok Build: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shaped(value: Value) -> Value {
        serde_json::from_slice(&request(value.to_string().as_bytes()).unwrap()).unwrap()
    }

    #[test]
    fn the_bookkeeping_the_proxy_never_issued_is_dropped() {
        let out = shaped(json!({
            "model": "grok-4", "input": "hi",
            "previous_response_id": "resp_1", "metadata": {"a": 1},
            "prompt_cache_retention": "24h", "safety_identifier": "s",
            "stream_options": {"include_usage": true}, "top_p": 0.0,
            "temperature": 0.4,
        }));
        for name in DROPPED {
            assert!(out.get(*name).is_none(), "{name}");
        }
        assert!(out.get("top_p").is_none());
        assert_eq!(out["temperature"], 0.4, "what it does accept survives");
        assert!(
            out.get("prompt_cache_key").is_none(),
            "only a composer model is given one"
        );
    }

    #[test]
    fn a_composer_model_is_given_the_cache_key_it_needs() {
        let out = shaped(json!({"model": "grok-composer-1", "input": "hi"}));
        let key = out["prompt_cache_key"].as_str().unwrap();
        assert_eq!(key.len(), 36);
        assert_eq!(
            cache_key(out.to_string().as_bytes()).as_deref(),
            Some(key),
            "the header mirrors the body"
        );

        let stated = shaped(json!({"model": "grok-composer-1", "input": "hi",
                                   "prompt_cache_key": "mine"}));
        assert_eq!(stated["prompt_cache_key"], "mine");
    }
}
