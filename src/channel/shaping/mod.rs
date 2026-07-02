//! Shared upstream-body shaping (整形) helpers.
//!
//! Pure, best-effort field hygiene reused across channels: each shaper operates
//! on a `serde_json::Value` (request bodies) or rewrites a buffered response
//! body. Channels compose these in their `shape_request` / `shape_response`
//! impls, dispatching on [`crate::channel::ShapeCtx`].
//!
//! Best-effort contract: a body shaper that can't parse its input returns the
//! original bytes unchanged (see [`with_json_body`]).

pub mod anthropic_beta;
pub mod claude_cache_control;
pub mod claude_fallback;
pub mod claude_magic_cache;
pub mod claude_sampling;
pub mod gemini_genconfig;
pub mod vertex_normalize;

use bytes::Bytes;
use serde_json::Value;

/// Parse `body` as a JSON [`Value`], apply `f`, and serialize back to [`Bytes`].
///
/// Best-effort: if the input does not parse as JSON, or the mutated value fails
/// to serialize, the original `body` is returned unchanged. This is the common
/// wrapper for the object-mutating shapers in this module.
pub fn with_json_body(body: Bytes, f: impl FnOnce(&mut Value)) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    f(&mut value);
    match serde_json::to_vec(&value) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn applies_mutation_and_reserializes() {
        let body = Bytes::from(r#"{"a":1}"#);
        let out = with_json_body(body, |v| {
            v.as_object_mut().unwrap().insert("b".into(), json!(2));
        });
        let parsed: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn returns_original_on_parse_failure() {
        let body = Bytes::from_static(b"not json");
        let out = with_json_body(body.clone(), |_| panic!("must not run"));
        assert_eq!(out, body);
    }
}
