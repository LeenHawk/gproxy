//! Content-generation-aware rule applications: system text injection and claude
//! cache breakpoints. These must know the provider-native body shape.

use serde_json::{Value, json};

use super::compile::{CacheBreakpointCfg, TextPosition};
use crate::protocol::ContentGenerationKind;

/// Insert or append system text in the target kind's native location.
pub fn system_text(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    text: &str,
    position: TextPosition,
) {
    use ContentGenerationKind as K;
    let Some(obj) = body.as_object_mut() else {
        return warn_skip("system_text", "body not an object");
    };
    match kind {
        Some(K::ClaudeMessages) => match obj.get_mut("system") {
            None | Some(Value::Null) => {
                obj.insert("system".to_owned(), json!(text));
            }
            Some(Value::String(s)) => match position {
                TextPosition::Prepend => *s = format!("{text} {s}"),
                TextPosition::Append => *s = format!("{s}\n\n{text}"),
            },
            Some(Value::Array(arr)) => match position {
                TextPosition::Prepend => arr.insert(0, json!({"type": "text", "text": text})),
                TextPosition::Append => arr.push(json!({"type": "text", "text": text})),
            },
            Some(_) => warn_skip("system_text", "unexpected claude system shape"),
        },
        Some(K::OpenAiChatCompletions) => match obj.get_mut("messages") {
            Some(Value::Array(msgs)) => match position {
                TextPosition::Prepend => {
                    msgs.insert(0, json!({"role": "system", "content": text}));
                }
                TextPosition::Append => {
                    // Insert after the leading run of system-role messages.
                    let insert_at = msgs
                        .iter()
                        .take_while(|m| m.get("role").and_then(Value::as_str) == Some("system"))
                        .count();
                    msgs.insert(insert_at, json!({"role": "system", "content": text}));
                }
            },
            _ => warn_skip("system_text", "missing messages array"),
        },
        Some(K::OpenAiResponses) | Some(K::OpenAiResponsesWebSocket) => {
            match obj.get_mut("instructions") {
                None | Some(Value::Null) => {
                    obj.insert("instructions".to_owned(), json!(text));
                }
                Some(Value::String(s)) => match position {
                    TextPosition::Prepend => *s = format!("{text} {s}"),
                    TextPosition::Append => *s = format!("{s}\n\n{text}"),
                },
                Some(_) => warn_skip("system_text", "unexpected instructions shape"),
            }
        }
        Some(K::GeminiGenerateContent) => {
            let part = json!({"text": text});
            match obj.get_mut("systemInstruction") {
                None | Some(Value::Null) => {
                    obj.insert("systemInstruction".to_owned(), json!({"parts": [part]}));
                }
                Some(Value::Object(si)) => match si.get_mut("parts") {
                    Some(Value::Array(parts)) => match position {
                        TextPosition::Prepend => parts.insert(0, part),
                        TextPosition::Append => parts.push(part),
                    },
                    _ => {
                        si.insert("parts".to_owned(), json!([part]));
                    }
                },
                Some(_) => warn_skip("system_text", "unexpected systemInstruction shape"),
            }
        }
        None => warn_skip("system_text", "non-content operation"),
    }
}

/// Insert a `cache_control` marker (claude wire only; other kinds skip).
pub fn cache_breakpoint(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    cfg: &CacheBreakpointCfg,
) {
    if kind != Some(ContentGenerationKind::ClaudeMessages) {
        return warn_skip("cache_breakpoint", "non-claude target");
    }
    let Some(obj) = body.as_object_mut() else {
        return warn_skip("cache_breakpoint", "body not an object");
    };
    let mut control = json!({"type": "ephemeral"});
    if let Some(ttl) = &cfg.ttl {
        control["ttl"] = json!(ttl);
    }
    // Top-level (global) breakpoint: stamp `cache_control` on the request root,
    // enabling Anthropic's automatic prompt caching. `index`/`position` are
    // irrelevant here. (v1 parity: `CacheBreakpointTarget::TopLevel`.)
    if matches!(cfg.target.as_str(), "top_level" | "global") {
        obj.entry("cache_control").or_insert(control);
        return;
    }
    let blocks: Option<&mut Vec<Value>> = match cfg.target.as_str() {
        // string-form `system` cannot carry block markers — skips via None
        "system" => obj.get_mut("system").and_then(Value::as_array_mut),
        "tools" => obj.get_mut("tools").and_then(Value::as_array_mut),
        "last_message" => obj
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .and_then(|m| m.last_mut())
            .and_then(|m| m.get_mut("content"))
            .and_then(Value::as_array_mut),
        _ => None,
    };
    let Some(blocks) = blocks else {
        return warn_skip("cache_breakpoint", "target not found or not a block array");
    };
    let Some(idx) = resolve_block_index(blocks.len(), cfg.index) else {
        return warn_skip("cache_breakpoint", "index out of range or invalid");
    };
    if let Some(Value::Object(block)) = blocks.get_mut(idx) {
        block.insert("cache_control".to_owned(), control);
    }
}

/// Resolve a console-facing **signed, 1-based** `index` against a block array of
/// length `len`: `>0` is the Nth block from the start, `<0` is the Nth from the
/// end, `0` is invalid. Omitted (`None`) defaults to the last block. Returns
/// `None` (skip) when the array is empty or the index is invalid/out of range.
fn resolve_block_index(len: usize, index: Option<i64>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match index {
        None => Some(len - 1),
        Some(0) => None,
        Some(i) if i > 0 => {
            let nth = i as usize;
            (nth <= len).then(|| nth - 1)
        }
        Some(i) => {
            let from_end = i.unsigned_abs() as usize;
            (from_end <= len).then(|| len - from_end)
        }
    }
}

fn warn_skip(rule: &str, reason: &str) {
    tracing::warn!(rule, reason, "process rule skipped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ContentGenerationKind as K;

    #[test]
    fn system_text_per_kind() {
        // --- prepend (default) ---
        let mut claude = json!({"system": "old", "messages": []});
        system_text(
            &mut claude,
            Some(K::ClaudeMessages),
            "P",
            TextPosition::Prepend,
        );
        assert_eq!(claude["system"], "P old");

        let mut chat = json!({"messages": [{"role": "user", "content": "hi"}]});
        system_text(
            &mut chat,
            Some(K::OpenAiChatCompletions),
            "P",
            TextPosition::Prepend,
        );
        assert_eq!(chat["messages"][0]["role"], "system");

        let mut gem = json!({"contents": []});
        system_text(
            &mut gem,
            Some(K::GeminiGenerateContent),
            "P",
            TextPosition::Prepend,
        );
        assert_eq!(gem["systemInstruction"]["parts"][0]["text"], "P");

        // --- append: claude string ---
        let mut claude2 = json!({"system": "old"});
        system_text(
            &mut claude2,
            Some(K::ClaudeMessages),
            "A",
            TextPosition::Append,
        );
        assert_eq!(claude2["system"], "old\n\nA");

        // --- append: chat messages with leading system run ---
        let mut chat2 = json!({"messages": [
            {"role": "system", "content": "s1"},
            {"role": "system", "content": "s2"},
            {"role": "user",   "content": "hi"}
        ]});
        system_text(
            &mut chat2,
            Some(K::OpenAiChatCompletions),
            "A",
            TextPosition::Append,
        );
        // new system message inserted at index 2 (after the 2 leading system messages)
        assert_eq!(chat2["messages"][2]["role"], "system");
        assert_eq!(chat2["messages"][2]["content"], "A");
        assert_eq!(chat2["messages"][3]["role"], "user");
    }

    #[test]
    fn cache_breakpoint_last_message() {
        let mut v = json!({"messages": [
            {"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}
        ]});
        let cfg = CacheBreakpointCfg {
            target: "last_message".into(),
            index: None,
            ttl: Some("5m".into()),
            position: None,
        };
        cache_breakpoint(&mut v, Some(K::ClaudeMessages), &cfg);
        assert_eq!(v["messages"][0]["content"][1]["cache_control"]["ttl"], "5m");
        assert!(
            v["messages"][0]["content"][0]
                .get("cache_control")
                .is_none()
        );
    }

    #[test]
    fn cache_breakpoint_top_level() {
        let mut v = json!({"system": "x", "messages": []});
        let cfg = CacheBreakpointCfg {
            target: "top_level".into(),
            index: None,
            ttl: Some("1h".into()),
            position: None,
        };
        cache_breakpoint(&mut v, Some(K::ClaudeMessages), &cfg);
        // Marker lands on the request root, not in a block array.
        assert_eq!(v["cache_control"]["type"], "ephemeral");
        assert_eq!(v["cache_control"]["ttl"], "1h");
    }

    #[test]
    fn resolve_block_index_signed_semantics() {
        // >0: 1-based from the start
        assert_eq!(resolve_block_index(3, Some(1)), Some(0));
        assert_eq!(resolve_block_index(3, Some(3)), Some(2));
        assert_eq!(resolve_block_index(3, Some(4)), None);
        // <0: 1-based from the end
        assert_eq!(resolve_block_index(3, Some(-1)), Some(2));
        assert_eq!(resolve_block_index(3, Some(-3)), Some(0));
        assert_eq!(resolve_block_index(3, Some(-4)), None);
        // 0 invalid; omitted → last; empty array → skip
        assert_eq!(resolve_block_index(3, Some(0)), None);
        assert_eq!(resolve_block_index(3, None), Some(2));
        assert_eq!(resolve_block_index(0, Some(1)), None);
    }
}
