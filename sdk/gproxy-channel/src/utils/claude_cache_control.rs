use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheBreakpointTarget {
    #[default]
    TopLevel,
    Tools,
    System,
    Messages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheBreakpointPositionKind {
    #[default]
    Nth,
    LastNth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheBreakpointTtl {
    #[default]
    Auto,
    #[serde(alias = "5m")]
    Ttl5m,
    #[serde(alias = "1h")]
    Ttl1h,
}

impl CacheBreakpointTtl {
    pub fn ttl(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Ttl5m => Some("5m"),
            Self::Ttl1h => Some("1h"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheBreakpointRule {
    pub target: CacheBreakpointTarget,
    #[serde(default)]
    pub position: CacheBreakpointPositionKind,
    #[serde(default = "default_cache_breakpoint_index")]
    pub index: usize,
    #[serde(default)]
    pub ttl: CacheBreakpointTtl,
}

impl CacheBreakpointRule {
    fn normalized(mut self) -> Self {
        if self.index == 0 {
            self.index = 1;
        }
        self
    }
}

fn default_cache_breakpoint_index() -> usize {
    1
}

const MAGIC_TRIGGER_AUTO_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_TRIGGER_5M_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_TRIGGER_1H_ID: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";

pub fn canonicalize_claude_body(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(system) = root.get_mut("system") {
        canonicalize_claude_system(system);
    }

    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            canonicalize_claude_message(message);
        }
    }
}

fn canonicalize_claude_system(system: &mut Value) {
    match system {
        Value::String(text) => {
            let text = std::mem::take(text);
            *system = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Array(blocks) => canonicalize_claude_blocks(blocks),
        _ => {}
    }
}

fn canonicalize_claude_message(message: &mut Value) {
    let Some(message_map) = message.as_object_mut() else {
        return;
    };
    let Some(content) = message_map.get_mut("content") else {
        return;
    };
    canonicalize_claude_content(content);
}

fn canonicalize_claude_content(content: &mut Value) {
    match content {
        Value::String(text) => {
            let text = std::mem::take(text);
            *content = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Object(_) => {
            let block = std::mem::take(content);
            *content = Value::Array(vec![block]);
        }
        Value::Array(blocks) => canonicalize_claude_blocks(blocks),
        _ => {}
    }
}

fn canonicalize_claude_blocks(blocks: &mut Vec<Value>) {
    for block in blocks {
        if let Value::String(text) = block {
            let text = std::mem::take(text);
            *block = json_text_block(text.as_str());
        }
    }
}

fn json_text_block(text: &str) -> Value {
    serde_json::json!({
        "type": "text",
        "text": text,
    })
}

/// Flatten consecutive text blocks in `system` into a single text block so
/// that downstream cache breakpoint logic has fewer, larger segments to
/// work with. Non-text blocks are preserved and serve as run boundaries.
/// If any text block in a run carries `cache_control`, the last such marker
/// is kept on the merged block.
pub fn flatten_system_text_blocks(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let Some(Value::Array(blocks)) = root.get_mut("system") else {
        return;
    };
    if blocks.len() <= 1 {
        return;
    }

    let owned = std::mem::take(blocks);
    let mut out: Vec<Value> = Vec::with_capacity(owned.len());
    let mut run_text = String::new();
    let mut run_cc: Option<Value> = None;

    let flush = |out: &mut Vec<Value>, text: &mut String, cc: &mut Option<Value>| {
        if text.is_empty() && cc.is_none() {
            return;
        }
        let mut merged = serde_json::Map::new();
        merged.insert("type".into(), Value::String("text".into()));
        merged.insert("text".into(), Value::String(std::mem::take(text)));
        if let Some(cc) = cc.take() {
            merged.insert("cache_control".into(), cc);
        }
        out.push(Value::Object(merged));
    };

    for block in owned {
        let Value::Object(map) = block else {
            flush(&mut out, &mut run_text, &mut run_cc);
            out.push(block);
            continue;
        };
        let is_text = map.get("type").and_then(Value::as_str) == Some("text");
        if !is_text {
            flush(&mut out, &mut run_text, &mut run_cc);
            out.push(Value::Object(map));
            continue;
        }
        let text = map.get("text").and_then(Value::as_str).unwrap_or("");
        run_text.push_str(text);
        if let Some(cc) = map.get("cache_control") {
            run_cc = Some(cc.clone());
        }
    }
    flush(&mut out, &mut run_text, &mut run_cc);

    if let Some(Value::Array(blocks)) = root.get_mut("system") {
        *blocks = out;
    }
}

pub fn apply_magic_string_cache_control_triggers(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let existing_breakpoints = existing_cache_breakpoint_count(root);
    let mut remaining_slots = 4usize.saturating_sub(existing_breakpoints);

    if let Some(system) = root.get_mut("system") {
        apply_magic_trigger_to_content(system, &mut remaining_slots);
    }

    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.as_object_mut().and_then(|m| m.get_mut("content")) {
                apply_magic_trigger_to_content(content, &mut remaining_slots);
            }
        }
    }
}

fn apply_magic_trigger_to_content(content: &mut Value, remaining_slots: &mut usize) {
    match content {
        Value::Array(blocks) => {
            for block in blocks {
                if let Some(map) = block.as_object_mut() {
                    strip_and_apply_magic_trigger(map, remaining_slots);
                }
            }
        }
        Value::Object(map) => {
            strip_and_apply_magic_trigger(map, remaining_slots);
        }
        _ => {}
    }
}

fn strip_and_apply_magic_trigger(
    block_map: &mut serde_json::Map<String, Value>,
    remaining_slots: &mut usize,
) {
    let Some(Value::String(text)) = block_map.get_mut("text") else {
        return;
    };
    let Some(ttl) = remove_magic_trigger_tokens(text) else {
        return;
    };
    if *remaining_slots > 0
        && !block_map.contains_key("cache_control")
        && block_supports_direct_cache_control(block_map)
    {
        block_map.insert("cache_control".to_string(), cache_control_ephemeral(ttl));
        *remaining_slots = remaining_slots.saturating_sub(1);
    }
}

/// Whether a content block can legally carry `cache_control` directly.
///
/// Anthropic rejects `cache_control` on `thinking` / `redacted_thinking`
/// blocks. Empty `text` blocks are allowed here because
/// `sanitize_claude_body` later drops them and shifts the marker onto the
/// previous anchor — rejecting them at injection time would lose that shift.
fn block_supports_direct_cache_control(block_map: &serde_json::Map<String, Value>) -> bool {
    !matches!(
        block_map.get("type").and_then(Value::as_str),
        Some("thinking" | "redacted_thinking")
    )
}

fn remove_magic_trigger_tokens(text: &mut String) -> Option<CacheBreakpointTtl> {
    let specs = [
        (MAGIC_TRIGGER_AUTO_ID, CacheBreakpointTtl::Auto),
        (MAGIC_TRIGGER_5M_ID, CacheBreakpointTtl::Ttl5m),
        (MAGIC_TRIGGER_1H_ID, CacheBreakpointTtl::Ttl1h),
    ];

    let mut matched_ttl = None;
    for (id, ttl) in specs {
        if text.contains(id) {
            *text = text.replace(id, "");
            if matched_ttl.is_none() {
                matched_ttl = Some(ttl);
            }
        }
    }

    matched_ttl
}

pub fn parse_cache_breakpoint_rules(value: Option<&Value>) -> Vec<CacheBreakpointRule> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(parse_cache_breakpoint_rule)
        .take(4)
        .collect()
}

fn parse_cache_breakpoint_rule(item: &Value) -> Option<CacheBreakpointRule> {
    let obj = item.as_object()?;
    let target = match obj
        .get("target")
        .and_then(Value::as_str)?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "global" | "top_level" => CacheBreakpointTarget::TopLevel,
        "tools" => CacheBreakpointTarget::Tools,
        "system" => CacheBreakpointTarget::System,
        "messages" => CacheBreakpointTarget::Messages,
        _ => return None,
    };

    let position = parse_cache_breakpoint_position(obj.get("position"));

    let index = obj
        .get("index")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(1);

    let ttl = match obj
        .get("ttl")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("auto")
        .to_ascii_lowercase()
        .as_str()
    {
        "5m" | "ttl5m" => CacheBreakpointTtl::Ttl5m,
        "1h" | "ttl1h" => CacheBreakpointTtl::Ttl1h,
        _ => CacheBreakpointTtl::Auto,
    };

    Some(
        CacheBreakpointRule {
            target,
            position,
            index,
            ttl,
        }
        .normalized(),
    )
}

fn parse_cache_breakpoint_position(value: Option<&Value>) -> CacheBreakpointPositionKind {
    match value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("nth")
        .to_ascii_lowercase()
        .as_str()
    {
        "last" | "last_nth" | "from_end" => CacheBreakpointPositionKind::LastNth,
        _ => CacheBreakpointPositionKind::Nth,
    }
}

pub fn cache_breakpoint_rules_to_settings_value(rules: &[CacheBreakpointRule]) -> Option<Value> {
    let normalized: Vec<CacheBreakpointRule> = rules
        .iter()
        .cloned()
        .map(CacheBreakpointRule::normalized)
        .take(4)
        .collect();
    if normalized.is_empty() {
        return None;
    }
    serde_json::to_value(normalized).ok()
}

pub fn ensure_cache_breakpoint_rules(body: &mut Value, rules: &[CacheBreakpointRule]) {
    if rules.is_empty() {
        return;
    }
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    let existing_breakpoints = existing_cache_breakpoint_count(root);
    let mut remaining_slots = 4usize.saturating_sub(existing_breakpoints);
    if remaining_slots == 0 {
        return;
    }

    for rule in rules.iter().take(4) {
        if remaining_slots == 0 {
            break;
        }
        apply_cache_breakpoint_rule(root, &rule.clone().normalized(), &mut remaining_slots);
    }
}

fn apply_cache_breakpoint_rule(
    root: &mut serde_json::Map<String, Value>,
    rule: &CacheBreakpointRule,
    remaining_slots: &mut usize,
) {
    if *remaining_slots == 0 {
        return;
    }

    match rule.target {
        CacheBreakpointTarget::TopLevel => {
            if !root.contains_key("cache_control") {
                root.insert(
                    "cache_control".to_string(),
                    cache_control_ephemeral(rule.ttl),
                );
                *remaining_slots = remaining_slots.saturating_sub(1);
            }
        }
        CacheBreakpointTarget::Tools => {
            let Some(tools) = root.get_mut("tools").and_then(Value::as_array_mut) else {
                return;
            };
            let Some(idx) = resolve_rule_index(tools.len(), rule.position, rule.index) else {
                return;
            };
            let Some(map) = tools[idx].as_object_mut() else {
                return;
            };
            if !map.contains_key("cache_control") {
                map.insert(
                    "cache_control".to_string(),
                    cache_control_ephemeral(rule.ttl),
                );
                *remaining_slots = remaining_slots.saturating_sub(1);
            }
        }
        CacheBreakpointTarget::System => match root.get_mut("system") {
            Some(Value::Array(blocks)) => {
                let Some(idx) = resolve_rule_index(blocks.len(), rule.position, rule.index) else {
                    return;
                };
                let Some(map) = blocks[idx].as_object_mut() else {
                    return;
                };
                if !map.contains_key("cache_control") {
                    map.insert(
                        "cache_control".to_string(),
                        cache_control_ephemeral(rule.ttl),
                    );
                    *remaining_slots = remaining_slots.saturating_sub(1);
                }
            }
            Some(Value::Object(map)) => {
                if resolve_rule_index(1, rule.position, rule.index).is_none() {
                    return;
                }
                if !map.contains_key("cache_control") {
                    map.insert(
                        "cache_control".to_string(),
                        cache_control_ephemeral(rule.ttl),
                    );
                    *remaining_slots = remaining_slots.saturating_sub(1);
                }
            }
            _ => {}
        },
        CacheBreakpointTarget::Messages => {
            let Some((message_idx, content_idx)) = root
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|messages| resolve_message_target_location(messages, rule))
            else {
                return;
            };
            let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
                return;
            };
            let Some(message_map) = messages.get_mut(message_idx).and_then(Value::as_object_mut)
            else {
                return;
            };
            let Some(content) = message_map.get_mut("content") else {
                return;
            };
            if apply_cache_control_to_message_block(content, content_idx, rule.ttl) {
                *remaining_slots = remaining_slots.saturating_sub(1);
            }
        }
    }
}

fn resolve_message_target_location(
    messages: &[Value],
    rule: &CacheBreakpointRule,
) -> Option<(usize, usize)> {
    resolve_message_block_location(messages, rule.position, rule.index)
}

fn resolve_message_block_location(
    messages: &[Value],
    position: CacheBreakpointPositionKind,
    index: usize,
) -> Option<(usize, usize)> {
    let mut locations = Vec::new();

    for (message_index, message) in messages.iter().enumerate() {
        let Some(message_map) = message.as_object() else {
            continue;
        };
        let Some(content) = message_map.get("content") else {
            continue;
        };

        match content {
            Value::Array(blocks) => {
                for (content_index, block) in blocks.iter().enumerate() {
                    if block.is_object() {
                        locations.push((message_index, content_index));
                    }
                }
            }
            Value::Object(_) => locations.push((message_index, 0)),
            _ => {}
        }
    }

    let idx = resolve_rule_index(locations.len(), position, index)?;
    locations.get(idx).copied()
}

fn apply_cache_control_to_message_block(
    content: &mut Value,
    content_idx: usize,
    ttl: CacheBreakpointTtl,
) -> bool {
    match content {
        Value::Array(blocks) => {
            let Some(map) = blocks.get_mut(content_idx).and_then(Value::as_object_mut) else {
                return false;
            };
            if !is_cacheable_block(map) {
                return false;
            }
            if map.contains_key("cache_control") {
                return false;
            }
            map.insert("cache_control".to_string(), cache_control_ephemeral(ttl));
            true
        }
        Value::Object(map) => {
            if content_idx != 0 {
                return false;
            }
            if !is_cacheable_block(map) {
                return false;
            }
            if map.contains_key("cache_control") {
                return false;
            }
            map.insert("cache_control".to_string(), cache_control_ephemeral(ttl));
            true
        }
        _ => false,
    }
}

/// Check if a content block can have cache_control applied.
///
/// Blocks that CANNOT be cached:
/// - `thinking` blocks (must be cached indirectly via the assistant turn)
/// - Sub-content blocks like `citations` (cache the top-level document instead)
/// - Empty `text` blocks
fn is_cacheable_block(block: &serde_json::Map<String, Value>) -> bool {
    let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
    match block_type {
        "thinking" => false,
        "citation" | "citations" | "char_location" | "page_location" | "content_block_location" => {
            false
        }
        "text" => {
            // Empty text blocks cannot be cached
            block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| !t.is_empty())
        }
        _ => true,
    }
}

fn resolve_rule_index(
    len: usize,
    position: CacheBreakpointPositionKind,
    index: usize,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let idx = index.max(1);
    match position {
        CacheBreakpointPositionKind::Nth => {
            if idx > len {
                None
            } else {
                Some(idx - 1)
            }
        }
        CacheBreakpointPositionKind::LastNth => {
            if idx > len {
                None
            } else {
                Some(len - idx)
            }
        }
    }
}

fn cache_control_ephemeral(ttl: CacheBreakpointTtl) -> Value {
    let mut cache_control = serde_json::json!({
        "type": "ephemeral",
    });
    if let Some(ttl) = ttl.ttl() {
        cache_control["ttl"] = serde_json::json!(ttl);
    }
    cache_control
}

fn existing_cache_breakpoint_count(root: &serde_json::Map<String, Value>) -> usize {
    let mut count = 0usize;
    if root.contains_key("cache_control") {
        count += 1;
    }

    if let Some(tools) = root.get("tools").and_then(Value::as_array) {
        count += tools
            .iter()
            .filter_map(Value::as_object)
            .filter(|item| item.contains_key("cache_control"))
            .count();
    }

    match root.get("system") {
        Some(Value::Array(blocks)) => {
            count += blocks
                .iter()
                .filter_map(Value::as_object)
                .filter(|item| {
                    item.contains_key("cache_control") && block_supports_direct_cache_control(item)
                })
                .count();
        }
        Some(Value::Object(item))
            if item.contains_key("cache_control") && block_supports_direct_cache_control(item) =>
        {
            count += 1;
        }
        _ => {}
    }

    if let Some(messages) = root.get("messages").and_then(Value::as_array) {
        for message in messages {
            let Some(message_map) = message.as_object() else {
                continue;
            };
            let Some(content) = message_map.get("content") else {
                continue;
            };
            match content {
                Value::Array(blocks) => {
                    count += blocks
                        .iter()
                        .filter_map(Value::as_object)
                        .filter(|item| {
                            item.contains_key("cache_control")
                                && block_supports_direct_cache_control(item)
                        })
                        .count();
                }
                Value::Object(item)
                    if item.contains_key("cache_control")
                        && block_supports_direct_cache_control(item) =>
                {
                    count += 1;
                }
                _ => {}
            }
        }
    }

    count
}

/// Remove whitespace-only text blocks, empty content arrays, and empty
/// messages. When a removed block carried `cache_control`, shift the marker
/// onto the most recent surviving cacheable block — first within the same
/// content/system array, then within previously kept messages. If no prior
/// cacheable block exists anywhere, the marker is dropped.
pub fn sanitize_claude_body(body: &mut Value) {
    canonicalize_claude_body(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(Value::Array(blocks)) = root.get_mut("system") {
        let owned = std::mem::take(blocks);
        let cleaned = sanitize_block_array(owned, &mut []);
        if cleaned.is_empty() {
            root.remove("system");
        } else if let Some(Value::Array(target)) = root.get_mut("system") {
            *target = cleaned;
        }
    }

    if let Some(Value::Array(messages)) = root.get_mut("messages") {
        let owned = std::mem::take(messages);
        let mut kept: Vec<Value> = Vec::with_capacity(owned.len());
        for mut message in owned {
            let Some(message_map) = message.as_object_mut() else {
                kept.push(message);
                continue;
            };
            let cleaned_content = match message_map.remove("content") {
                Some(Value::Array(blocks)) => sanitize_block_array(blocks, kept.as_mut_slice()),
                Some(other) => {
                    message_map.insert("content".into(), other);
                    kept.push(Value::Object(message_map.clone()));
                    continue;
                }
                None => {
                    kept.push(Value::Object(message_map.clone()));
                    continue;
                }
            };
            if cleaned_content.is_empty() {
                continue;
            }
            message_map.insert("content".into(), Value::Array(cleaned_content));
            kept.push(Value::Object(message_map.clone()));
        }
        if let Some(Value::Array(target)) = root.get_mut("messages") {
            *target = kept;
        }
    }
}

fn sanitize_block_array(blocks: Vec<Value>, prev_messages: &mut [Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let Value::Object(mut map) = block else {
            out.push(block);
            continue;
        };
        let is_text = map.get("type").and_then(Value::as_str) == Some("text");
        if is_text {
            let trimmed = map
                .get("text")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string());
            if let Some(t) = trimmed {
                if t.is_empty() {
                    if let Some(cc) = map.remove("cache_control")
                        && !attach_cc_to_prev_in_scope(&mut out, &cc)
                    {
                        attach_cc_to_prev_messages(prev_messages, &cc);
                    }
                    continue;
                }
                map.insert("text".into(), Value::String(t));
            }
        }
        out.push(Value::Object(map));
    }
    out
}

fn attach_cc_to_prev_in_scope(out: &mut [Value], cc: &Value) -> bool {
    for block in out.iter_mut().rev() {
        let Some(map) = block.as_object_mut() else {
            continue;
        };
        if !is_cacheable_block(map) {
            continue;
        }
        if !map.contains_key("cache_control") {
            map.insert("cache_control".into(), cc.clone());
        }
        return true;
    }
    false
}

fn attach_cc_to_prev_messages(messages: &mut [Value], cc: &Value) -> bool {
    for message in messages.iter_mut().rev() {
        let Some(map) = message.as_object_mut() else {
            continue;
        };
        let Some(Value::Array(blocks)) = map.get_mut("content") else {
            continue;
        };
        if attach_cc_to_prev_in_scope(blocks.as_mut_slice(), cc) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod sanitize_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_empty_user_text_block_and_message() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": ""},
                {"role": "user", "content": "hi"}
            ]
        });
        sanitize_claude_body(&mut body);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"][0]["text"], "hi");
    }

    #[test]
    fn drops_whitespace_only_text_block() {
        let mut body = json!({
            "system": [
                {"type": "text", "text": "   \n"},
                {"type": "text", "text": "real"}
            ]
        });
        sanitize_claude_body(&mut body);
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "real");
    }

    #[test]
    fn shifts_cache_control_to_prev_block_in_same_array() {
        let mut body = json!({
            "system": [
                {"type": "text", "text": "anchor"},
                {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
            ]
        });
        sanitize_claude_body(&mut body);
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "anchor");
        assert_eq!(sys[0]["cache_control"]["ttl"], "5m");
    }

    #[test]
    fn shifts_cache_control_across_messages() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "first"}]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": " ", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        let block = &messages[0]["content"][0];
        assert_eq!(block["text"], "first");
        assert_eq!(block["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn drops_cc_when_no_prior_cacheable_block_exists() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        assert!(body["messages"].as_array().unwrap().is_empty());
    }

    #[test]
    fn removes_system_field_when_all_blocks_drop() {
        let mut body = json!({
            "system": [{"type": "text", "text": "  "}],
            "messages": [{"role": "user", "content": "hi"}]
        });
        sanitize_claude_body(&mut body);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn preserves_non_text_blocks() {
        let mut body = json!({
            "messages": [
                {"role": "user", "content": [
                    {"type": "image", "source": {"type": "base64", "data": "x"}},
                    {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        });
        sanitize_claude_body(&mut body);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn magic_trigger_then_sanitize_shifts_cc() {
        let mut body = json!({
            "system": [
                {"type": "text", "text": "anchor"},
                {"type": "text", "text": MAGIC_TRIGGER_5M_ID}
            ]
        });
        apply_magic_string_cache_control_triggers(&mut body);
        sanitize_claude_body(&mut body);
        let sys = body["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "anchor");
        assert_eq!(sys[0]["cache_control"]["ttl"], "5m");
    }

    #[test]
    fn trims_text_when_kept() {
        let mut body = json!({
            "messages": [{"role": "user", "content": "  hi  "}]
        });
        sanitize_claude_body(&mut body);
        assert_eq!(body["messages"][0]["content"][0]["text"], "hi");
    }
}
