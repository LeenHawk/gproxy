//! Content-generation-aware rule applications: system text injection and
//! provider-native cache breakpoints.

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

/// Insert the target protocol's native cache marker.
pub fn cache_breakpoint(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    cfg: &CacheBreakpointCfg,
) {
    if matches!(
        kind,
        Some(
            ContentGenerationKind::OpenAiChatCompletions
                | ContentGenerationKind::OpenAiResponses
                | ContentGenerationKind::OpenAiResponsesWebSocket
        )
    ) {
        if cfg.ttl.as_deref().is_some_and(|ttl| ttl != "30m") {
            tracing::warn!(
                rule = "cache_breakpoint",
                ttl = cfg.ttl.as_deref(),
                "OpenAI only supports request-wide cache ttl 30m; ttl ignored"
            );
        }
        if let Err(reason) = crate::channel::shaping::openai_cache::apply_manual_cache_breakpoint(
            body,
            kind.expect("matched OpenAI kind"),
            &cfg.target,
            cfg.index,
            cfg.ttl.as_deref(),
        ) {
            warn_cache_skip(cfg, reason);
        }
        return;
    }
    if kind != Some(ContentGenerationKind::ClaudeMessages) {
        return warn_cache_skip(cfg, "unsupported target protocol");
    }
    if let Err(reason) =
        crate::channel::shaping::claude_cache_control::apply_manual_cache_breakpoint(
            body,
            &cfg.target,
            cfg.index,
            cfg.ttl.as_deref(),
        )
    {
        warn_cache_skip(cfg, reason);
    }
}

fn warn_cache_skip(cfg: &CacheBreakpointCfg, reason: &str) {
    tracing::warn!(
        rule = "cache_breakpoint",
        target = %cfg.target,
        index = ?cfg.index,
        reason,
        "process rule skipped"
    );
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
    fn cache_breakpoint_message() {
        let mut v = json!({"messages": [
            {"role": "user", "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}
        ]});
        let cfg = CacheBreakpointCfg {
            target: "message".into(),
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
    fn cache_breakpoint_openai_chat_system_string() {
        let mut v = json!({"messages": [
            {"role": "system", "content": "stable"},
            {"role": "user", "content": "hello"}
        ]});
        let cfg = CacheBreakpointCfg {
            target: "system".into(),
            index: None,
            ttl: Some("30m".into()),
            position: None,
        };
        cache_breakpoint(&mut v, Some(K::OpenAiChatCompletions), &cfg);
        assert_eq!(
            v["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(v["prompt_cache_options"]["ttl"], "30m");
    }

    #[test]
    fn cache_breakpoint_openai_responses_instructions() {
        let mut v = json!({"instructions": "stable", "input": "hello"});
        let cfg = CacheBreakpointCfg {
            target: "system".into(),
            index: None,
            ttl: None,
            position: None,
        };
        cache_breakpoint(&mut v, Some(K::OpenAiResponses), &cfg);
        assert_eq!(v["input"][0]["role"], "developer");
        assert_eq!(
            v["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(v["input"][1]["content"][0]["text"], "hello");
    }
}
