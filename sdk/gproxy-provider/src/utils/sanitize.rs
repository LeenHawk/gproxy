use gproxy_protocol::kinds::ProtocolKind;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single pattern → replacement rule for sanitizing request body text.
///
/// The `pattern` field is a regex (use `\b` for word boundaries to avoid
/// false positives on substrings like `pipeline`, `api`, etc.).
/// The `replacement` field is the literal substitution text.
///
/// Rules are applied in declaration order — put longer phrases first,
/// then shorter single-word patterns last, so the short patterns don't
/// prematurely break the longer matches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizeRule {
    pub pattern: String,
    pub replacement: String,
}

/// Apply sanitize rules to request body text, dispatching to the correct
/// field-walker based on the destination protocol.
///
/// | Protocol | Fields sanitized |
/// |----------|-----------------|
/// | Claude | `system`, `messages[*].content` |
/// | OpenAiChatCompletion / OpenAi | `messages[*].content` |
/// | OpenAiResponse | `instructions`, `input` (string or item array) |
/// | Gemini / GeminiNDJson | `systemInstruction.parts[*].text`, `contents[*].parts[*].text` |
pub fn apply_sanitize_rules(body: &mut Value, protocol: ProtocolKind, rules: &[SanitizeRule]) {
    if rules.is_empty() {
        return;
    }
    let compiled = compile_rules(rules);
    if compiled.is_empty() {
        return;
    }
    let Some(map) = body.as_object_mut() else {
        return;
    };

    match protocol {
        ProtocolKind::Claude => sanitize_claude(map, &compiled),
        ProtocolKind::OpenAiChatCompletion | ProtocolKind::OpenAi => {
            sanitize_openai_chat(map, &compiled)
        }
        ProtocolKind::OpenAiResponse => sanitize_openai_response(map, &compiled),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => sanitize_gemini(map, &compiled),
    }
}

// ---------------------------------------------------------------------------
// Protocol-specific walkers
// ---------------------------------------------------------------------------

fn sanitize_claude(map: &mut serde_json::Map<String, Value>, rules: &[(Regex, &str)]) {
    if let Some(system) = map.get_mut("system") {
        sanitize_value(rules, system);
    }
    sanitize_messages(map, rules);
}

fn sanitize_openai_chat(map: &mut serde_json::Map<String, Value>, rules: &[(Regex, &str)]) {
    sanitize_messages(map, rules);
}

fn sanitize_openai_response(map: &mut serde_json::Map<String, Value>, rules: &[(Regex, &str)]) {
    if let Some(instructions) = map.get_mut("instructions") {
        sanitize_value(rules, instructions);
    }
    if let Some(input) = map.get_mut("input") {
        match input {
            Value::String(_) => sanitize_value(rules, input),
            Value::Array(items) => {
                for item in items {
                    if let Some(content) = item.get_mut("content") {
                        sanitize_value(rules, content);
                    }
                    if let Some(output) = item.get_mut("output") {
                        sanitize_value(rules, output);
                    }
                }
            }
            _ => {}
        }
    }
}

fn sanitize_gemini(map: &mut serde_json::Map<String, Value>, rules: &[(Regex, &str)]) {
    if let Some(sys) = map.get_mut("systemInstruction") {
        sanitize_gemini_parts(rules, sys);
    }
    if let Some(Value::Array(contents)) = map.get_mut("contents") {
        for content in contents {
            sanitize_gemini_parts(rules, content);
        }
    }
    // generationConfig.contents (if present)
    if let Some(gen_config) = map.get_mut("generationConfig") {
        if let Some(Value::Array(contents)) = gen_config.get_mut("contents") {
            for content in contents {
                sanitize_gemini_parts(rules, content);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn compile_rules(rules: &[SanitizeRule]) -> Vec<(Regex, &str)> {
    rules
        .iter()
        .filter_map(|rule| {
            Regex::new(&rule.pattern)
                .ok()
                .map(|re| (re, rule.replacement.as_str()))
        })
        .collect()
}

fn sanitize_messages(map: &mut serde_json::Map<String, Value>, rules: &[(Regex, &str)]) {
    if let Some(Value::Array(messages)) = map.get_mut("messages") {
        for message in messages {
            if let Some(content) = message.get_mut("content") {
                sanitize_value(rules, content);
            }
        }
    }
}

fn sanitize_gemini_parts(rules: &[(Regex, &str)], container: &mut Value) {
    if let Some(Value::Array(parts)) = container.get_mut("parts") {
        for part in parts {
            if let Some(text) = part.get_mut("text") {
                sanitize_value(rules, text);
            }
        }
    }
}

fn sanitize_value(rules: &[(Regex, &str)], value: &mut Value) {
    match value {
        Value::String(text) => {
            for (re, replacement) in rules {
                let result = re.replace_all(text.as_str(), *replacement);
                if result != text.as_str() {
                    *text = result.into_owned();
                }
            }
        }
        Value::Array(blocks) => {
            for block in blocks {
                let current = match block.get("text").and_then(Value::as_str) {
                    Some(t) if !t.is_empty() => t.to_string(),
                    _ => continue,
                };
                let mut replaced = current;
                for (re, replacement) in rules {
                    let result = re.replace_all(&replaced, *replacement);
                    if result != replaced.as_str() {
                        replaced = result.into_owned();
                    }
                }
                if let Some(field) = block.get_mut("text") {
                    *field = Value::String(replaced);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pi_rules() -> Vec<SanitizeRule> {
        vec![
            SanitizeRule {
                pattern: r"\bPi documentation\b".to_string(),
                replacement: "Harness documentation".to_string(),
            },
            SanitizeRule {
                pattern: r"\bpi\b".to_string(),
                replacement: "the agent".to_string(),
            },
            SanitizeRule {
                pattern: r"\bPi\b".to_string(),
                replacement: "The agent".to_string(),
            },
        ]
    }

    #[test]
    fn sanitize_claude_system_string() {
        let rules = pi_rules();
        let mut body = json!({
            "system": "You are inside pi, a coding agent.",
            "messages": [{"role": "user", "content": "hello"}]
        });
        apply_sanitize_rules(&mut body, ProtocolKind::Claude, &rules);
        assert_eq!(
            body["system"].as_str().unwrap(),
            "You are inside the agent, a coding agent."
        );
    }

    #[test]
    fn sanitize_claude_system_array() {
        let rules = vec![SanitizeRule {
            pattern: r"\bAider\b".to_string(),
            replacement: "The assistant".to_string(),
        }];
        let mut body = json!({
            "system": [{"type": "text", "text": "Aider is a CLI tool."}],
            "messages": []
        });
        apply_sanitize_rules(&mut body, ProtocolKind::Claude, &rules);
        assert_eq!(
            body["system"][0]["text"].as_str().unwrap(),
            "The assistant is a CLI tool."
        );
    }

    #[test]
    fn sanitize_does_not_match_substrings() {
        let rules = vec![SanitizeRule {
            pattern: r"\bpi\b".to_string(),
            replacement: "agent".to_string(),
        }];
        let mut body = json!({
            "system": "Use the pipeline api to implement happy features.",
            "messages": []
        });
        apply_sanitize_rules(&mut body, ProtocolKind::Claude, &rules);
        assert_eq!(
            body["system"].as_str().unwrap(),
            "Use the pipeline api to implement happy features."
        );
    }

    #[test]
    fn longer_rules_applied_before_shorter() {
        let rules = pi_rules();
        let mut body = json!({
            "system": "Pi documentation is available. Pi is great.",
            "messages": []
        });
        apply_sanitize_rules(&mut body, ProtocolKind::Claude, &rules);
        assert_eq!(
            body["system"].as_str().unwrap(),
            "Harness documentation is available. The agent is great."
        );
    }

    #[test]
    fn sanitize_openai_chat() {
        let rules = vec![SanitizeRule {
            pattern: r"\bCline\b".to_string(),
            replacement: "Assistant".to_string(),
        }];
        let mut body = json!({
            "messages": [
                {"role": "system", "content": "You are Cline."},
                {"role": "user", "content": "Cline, help."}
            ]
        });
        apply_sanitize_rules(&mut body, ProtocolKind::OpenAiChatCompletion, &rules);
        assert_eq!(body["messages"][0]["content"].as_str().unwrap(), "You are Assistant.");
        assert_eq!(body["messages"][1]["content"].as_str().unwrap(), "Assistant, help.");
    }

    #[test]
    fn sanitize_openai_response() {
        let rules = vec![SanitizeRule {
            pattern: r"\bCursor\b".to_string(),
            replacement: "Editor".to_string(),
        }];
        let mut body = json!({
            "instructions": "You are Cursor.",
            "input": "Cursor can do things."
        });
        apply_sanitize_rules(&mut body, ProtocolKind::OpenAiResponse, &rules);
        assert_eq!(body["instructions"].as_str().unwrap(), "You are Editor.");
        assert_eq!(body["input"].as_str().unwrap(), "Editor can do things.");
    }

    #[test]
    fn sanitize_gemini() {
        let rules = vec![SanitizeRule {
            pattern: r"\bContinue\b".to_string(),
            replacement: "Assistant".to_string(),
        }];
        let mut body = json!({
            "systemInstruction": {"parts": [{"text": "You are Continue."}]},
            "contents": [{"role": "user", "parts": [{"text": "Continue, write code."}]}]
        });
        apply_sanitize_rules(&mut body, ProtocolKind::Gemini, &rules);
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"].as_str().unwrap(),
            "You are Assistant."
        );
        assert_eq!(
            body["contents"][0]["parts"][0]["text"].as_str().unwrap(),
            "Assistant, write code."
        );
    }
}
