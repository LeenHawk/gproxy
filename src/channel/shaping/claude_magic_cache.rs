//! Opt-in magic-string cache triggers for Claude request bodies.
//!
//! Ported from v1 `utils/claude_cache_control.rs`: a client embeds one of three
//! frozen magic strings inside its prompt text to mark a cache breakpoint; the
//! proxy strips the token and stamps `cache_control` (ephemeral, with the
//! matching ttl) on that block, capped at Claude's 4 breakpoints. Gated
//! per-provider by the `enable_magic_cache` setting.

use bytes::Bytes;
use serde_json::{Value, json};

use super::claude_cache_control::{canonicalize_claude_body, sanitize_claude_body};

// Frozen magic trigger strings — kept byte-for-byte in sync with v1
// `sdk/gproxy-channel/src/utils/claude_cache_control.rs`. A client embeds one
// verbatim, so these must match exactly.
const MAGIC_TRIGGER_AUTO_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_TRIGGER_5M_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_TRIGGER_1H_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";

#[derive(Clone, Copy)]
enum MagicTtl {
    Auto,
    Ttl5m,
    Ttl1h,
}

impl MagicTtl {
    fn as_str(self) -> Option<&'static str> {
        match self {
            MagicTtl::Auto => None,
            MagicTtl::Ttl5m => Some("5m"),
            MagicTtl::Ttl1h => Some("1h"),
        }
    }
}

/// Apply magic-string cache triggers to `body` only when the provider's
/// `enable_magic_cache` setting is true. Runs the strip+stamp pass, then
/// `sanitize_claude_body` to migrate any marker off a now-empty block. Returns
/// the body unchanged when disabled or unparseable.
pub fn apply_if_enabled(body: Bytes, settings: &Value) -> Bytes {
    let enabled = settings
        .get("enable_magic_cache")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !enabled {
        return body;
    }
    super::with_json_body(body, |v| {
        apply_magic_string_cache_control_triggers(v);
        sanitize_claude_body(v);
    })
}

/// Strip embedded magic trigger strings and stamp `cache_control` on the blocks
/// that carried them (up to Claude's 4-breakpoint cap, counting existing
/// markers). The caller runs `sanitize_claude_body` afterward.
pub fn apply_magic_string_cache_control_triggers(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let mut remaining = 4usize.saturating_sub(existing_cache_breakpoint_count(root));

    if let Some(system) = root.get_mut("system") {
        apply_to_content(system, &mut remaining);
    }
    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.as_object_mut().and_then(|m| m.get_mut("content")) {
                apply_to_content(content, &mut remaining);
            }
        }
    }
}

fn apply_to_content(content: &mut Value, remaining: &mut usize) {
    match content {
        Value::Array(blocks) => {
            for block in blocks {
                if let Some(map) = block.as_object_mut() {
                    strip_and_apply(map, remaining);
                }
            }
        }
        Value::Object(map) => strip_and_apply(map, remaining),
        _ => {}
    }
}

fn strip_and_apply(block: &mut serde_json::Map<String, Value>, remaining: &mut usize) {
    let Some(Value::String(text)) = block.get_mut("text") else {
        return;
    };
    let Some(ttl) = remove_magic_tokens(text) else {
        return;
    };
    if *remaining > 0 && !block.contains_key("cache_control") && supports_cache_control(block) {
        block.insert("cache_control".into(), cache_control_ephemeral(ttl));
        *remaining -= 1;
    }
}

/// Strip every magic token present; return the ttl of the first one matched
/// (declaration order), or `None` if the text carried no token.
fn remove_magic_tokens(text: &mut String) -> Option<MagicTtl> {
    let specs = [
        (MAGIC_TRIGGER_AUTO_ID, MagicTtl::Auto),
        (MAGIC_TRIGGER_5M_ID, MagicTtl::Ttl5m),
        (MAGIC_TRIGGER_1H_ID, MagicTtl::Ttl1h),
    ];
    let mut matched = None;
    for (id, ttl) in specs {
        if text.contains(id) {
            *text = text.replace(id, "");
            matched.get_or_insert(ttl);
        }
    }
    matched
}

/// Anthropic rejects `cache_control` on thinking blocks. Empty `text` blocks are
/// allowed here — the post-pass `sanitize_claude_body` shifts the marker onto the
/// previous anchor and drops the empty block.
fn supports_cache_control(block: &serde_json::Map<String, Value>) -> bool {
    !matches!(
        block.get("type").and_then(Value::as_str),
        Some("thinking" | "redacted_thinking")
    )
}

fn cache_control_ephemeral(ttl: MagicTtl) -> Value {
    match ttl.as_str() {
        Some(t) => json!({ "type": "ephemeral", "ttl": t }),
        None => json!({ "type": "ephemeral" }),
    }
}

fn existing_cache_breakpoint_count(root: &serde_json::Map<String, Value>) -> usize {
    let mut count = 0;
    if let Some(Value::Array(blocks)) = root.get("system") {
        count += blocks.iter().filter(|b| has_cache_control(b)).count();
    }
    if let Some(Value::Array(messages)) = root.get("messages") {
        for m in messages {
            if let Some(Value::Array(blocks)) = m.get("content") {
                count += blocks.iter().filter(|b| has_cache_control(b)).count();
            }
        }
    }
    count
}

fn has_cache_control(block: &Value) -> bool {
    block.get("cache_control").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_token_and_stamps_matching_ttl() {
        let mut body = json!({
            "system": [{ "type": "text", "text": format!("hello {MAGIC_TRIGGER_1H_ID} world") }]
        });
        apply_magic_string_cache_control_triggers(&mut body);
        sanitize_claude_body(&mut body);
        let blk = &body["system"][0];
        assert_eq!(blk["text"], "hello  world");
        assert_eq!(blk["cache_control"]["type"], "ephemeral");
        assert_eq!(blk["cache_control"]["ttl"], "1h");
    }

    #[test]
    fn auto_has_no_ttl_and_caps_at_four() {
        let blocks: Vec<Value> = (0..5)
            .map(|i| json!({ "type": "text", "text": format!("blk{i} {MAGIC_TRIGGER_AUTO_ID}") }))
            .collect();
        let mut body = json!({ "system": blocks });
        apply_magic_string_cache_control_triggers(&mut body);
        let stamped = body["system"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b.get("cache_control").is_some())
            .count();
        assert_eq!(stamped, 4); // 4-breakpoint cap
        let cc = &body["system"][0]["cache_control"];
        assert_eq!(cc["type"], "ephemeral");
        assert!(cc.get("ttl").is_none()); // auto → no ttl
    }

    #[test]
    fn disabled_setting_is_noop() {
        let body = Bytes::from_static(
            br#"{"system":[{"type":"text","text":"GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH"}]}"#,
        );
        let out = apply_if_enabled(body.clone(), &json!({}));
        assert_eq!(out, body); // unchanged when setting absent
    }
}
