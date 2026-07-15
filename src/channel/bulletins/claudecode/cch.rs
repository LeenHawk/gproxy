//! ClaudeCode CCH — the `x-anthropic-billing-header` checksum Claude Code embeds
//! in `system[0]` of every `/v1/messages` body. See `docs/claudecode-cch.md`.
//!
//! This intentionally matches the gproxy v1 / Claude Code 2.1.112 behavior:
//! 1. Inject `metadata.user_id` = the JSON-string `{device_id, account_uuid,
//!    session_id}` the CLI sends.
//! 2. Prepend a `system[0]` text block holding the billing header with a dynamic
//!    `cc_version` suffix derived from the first user text.
//! 3. Keep `cch=00000;` unchanged; this client version does not apply a
//!    post-serialization checksum rewrite.

use std::sync::OnceLock;

use dashmap::DashMap;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// Keep in lockstep with the channel User-Agent version.
const CLI_VERSION: &str = "2.1.112";
const CLI_ENTRYPOINT: &str = "cli";
/// Salt used by Claude Code to derive the three-hex `cc_version` suffix.
const CC_VERSION_SUFFIX_SALT: &str = "59cf53e54c78";

/// Rewrite the outbound `/v1/messages` body to carry the CLI's billing header +
/// `metadata.user_id`, with the v1-compatible literal `cch=00000`. `session_id`
/// is the value also sent as `x-claude-code-session-id`. Non-object bodies are
/// returned unchanged.
pub(super) fn apply(body: &[u8], device_id: &str, account_uuid: &str, session_id: &str) -> Vec<u8> {
    let Ok(mut v) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(obj) = v.as_object_mut() else {
        return body.to_vec();
    };

    // 1. metadata.user_id = JSON string of {device_id, account_uuid, session_id}.
    let user_id = json!({
        "device_id": device_id,
        "account_uuid": account_uuid,
        "session_id": session_id,
    })
    .to_string();
    let metadata = obj
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(m) = metadata.as_object_mut() {
        m.insert("user_id".into(), Value::String(user_id));
    }

    // 2. Prepend the billing-header block to `system` (literal v1 cch).
    let cc_version = cc_version(body);
    let billing = json!({
        "type": "text",
        "text": format!("x-anthropic-billing-header: cc_version={cc_version}; cc_entrypoint={CLI_ENTRYPOINT}; cch=00000;"),
    });
    match obj.get_mut("system") {
        // Replace an existing billing-header block in place (idempotent — a
        // re-proxied claude-code body already carries one); else prepend.
        Some(Value::Array(arr)) => {
            if let Some(b) = arr.iter_mut().find(|b| is_billing_block(b)) {
                *b = billing;
            } else {
                arr.insert(0, billing);
            }
        }
        Some(s @ Value::String(_)) => {
            let orig = s.take();
            *s = Value::Array(vec![billing, json!({ "type": "text", "text": orig })]);
        }
        _ => {
            obj.insert("system".into(), Value::Array(vec![billing]));
        }
    }

    // 3. Claude Code 2.1.112 sends the five zeroes unchanged.
    serde_json::to_vec(&v).unwrap_or_else(|_| body.to_vec())
}

fn cc_version(body: &[u8]) -> String {
    format!("{CLI_VERSION}.{}", cc_version_suffix(body))
}

fn cc_version_suffix(body: &[u8]) -> String {
    let text = first_user_text(body);
    let chars: Vec<char> = text.chars().collect();
    let picked: String = [4usize, 7, 20]
        .into_iter()
        .map(|idx| chars.get(idx).copied().unwrap_or('0'))
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(CC_VERSION_SUFFIX_SALT.as_bytes());
    hasher.update(picked.as_bytes());
    hasher.update(CLI_VERSION.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(3);
    for byte in digest.iter().take(2) {
        out.push_str(&format!("{byte:02x}"));
    }
    out.truncate(3);
    out
}

fn first_user_text(body: &[u8]) -> String {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return String::new();
    };
    let Some(messages) = v.get("messages").and_then(Value::as_array) else {
        return String::new();
    };
    for msg in messages {
        if msg.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = msg.get("content") else {
            continue;
        };
        if let Some(text) = content.as_str() {
            return text.to_owned();
        }
        if let Some(blocks) = content.as_array() {
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = block.get("text").and_then(Value::as_str)
                {
                    return text.to_owned();
                }
            }
        }
    }
    String::new()
}

/// Whether a `system` block already carries the billing header — so we replace
/// it in place rather than prepend a duplicate.
fn is_billing_block(b: &Value) -> bool {
    b.get("text")
        .and_then(Value::as_str)
        .is_some_and(|t| t.contains("x-anthropic-billing-header"))
}

/// v1 approximates a Claude Code process by reusing one random session UUID per
/// credential for 20 minutes.
const SESSION_ID_TTL_MS: u64 = 20 * 60 * 1000;

fn session_cache() -> &'static DashMap<String, (String, u64)> {
    static CACHE: OnceLock<DashMap<String, (String, u64)>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

/// Prefer an explicit downstream session id; otherwise reuse a random v4 UUID
/// for this credential until the v1 20-minute process window expires.
pub(super) fn session_id(device_id: &str, explicit: Option<&str>, now_ms: u64) -> String {
    if let Some(session_id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return session_id.to_owned();
    }

    let cache = session_cache();
    if let Some(entry) = cache.get(device_id) {
        let (session_id, created_at_ms) = entry.value();
        if now_ms.saturating_sub(*created_at_ms) < SESSION_ID_TTL_MS {
            return session_id.clone();
        }
    }

    let session_id = crate::util::rand::uuid_v4();
    cache.insert(device_id.to_owned(), (session_id.clone(), now_ms));
    session_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc_version_matches_v1_vector() {
        let body = br#"{"messages":[{"role":"user","content":"reply with exactly: ok"}]}"#;
        assert_eq!(cc_version(body), "2.1.112.b57");
    }

    #[test]
    fn apply_injects_metadata_and_literal_zero_cch() {
        let out = apply(
            br#"{"model":"claude-sonnet-4","messages":[]}"#,
            "devhash",
            "acct-1",
            "sess-uuid",
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        // metadata.user_id is the JSON-string of the three ids.
        let uid = v["metadata"]["user_id"].as_str().unwrap();
        let ids: Value = serde_json::from_str(uid).unwrap();
        assert_eq!(ids["device_id"], "devhash");
        assert_eq!(ids["account_uuid"], "acct-1");
        assert_eq!(ids["session_id"], "sess-uuid");
        // system[0] carries the v1 billing header with its literal zero cch.
        let txt = v["system"][0]["text"].as_str().unwrap();
        assert!(txt.contains("cc_version=2.1.112.b02"));
        assert!(txt.contains("cc_entrypoint=cli"));
        assert!(txt.contains("cch=00000;"));
    }

    #[test]
    fn session_id_reuses_v4_for_v1_process_window() {
        let a = session_id("v1-window-dev", None, 1_000_000);
        assert_eq!(a, session_id("v1-window-dev", None, 1_000_000 + 100));
        assert_eq!(a.len(), 36);
        assert_eq!(a.as_bytes()[14], b'4'); // version nibble
        assert_ne!(
            a,
            session_id("v1-window-dev", None, 1_000_000 + SESSION_ID_TTL_MS)
        );
    }

    #[test]
    fn session_id_prefers_explicit_value() {
        assert_eq!(
            session_id("explicit-dev", Some("caller-session"), 1_000_000),
            "caller-session"
        );
    }

    #[test]
    fn apply_replaces_existing_billing_block() {
        // Re-proxy case: the inbound body already carries a billing block.
        let body = br#"{"system":[{"type":"text","text":"x-anthropic-billing-header: cc_version=old; cc_entrypoint=x; cch=fffff;"},{"type":"text","text":"real system"}],"messages":[]}"#;
        let out = apply(body, "d", "a", "s");
        let v: Value = serde_json::from_slice(&out).unwrap();
        let sys = v["system"].as_array().unwrap();
        // Replaced in place, not duplicated → still exactly one billing block.
        assert_eq!(sys.iter().filter(|b| is_billing_block(b)).count(), 1);
        // The original non-billing block survives.
        assert!(sys.iter().any(|b| b["text"] == "real system"));
        // Our version replaces the stale block with the v1 billing shape.
        let txt = sys.iter().find(|b| is_billing_block(b)).unwrap()["text"]
            .as_str()
            .unwrap();
        assert!(txt.contains("cc_version=2.1.112.b02"));
        assert!(txt.contains("cch=00000"));
        assert!(!txt.contains("cch=fffff"));
    }
}
