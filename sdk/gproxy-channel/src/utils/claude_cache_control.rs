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
            if let Some(content) = message
                .as_object_mut()
                .and_then(|m| m.get_mut("content"))
            {
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
    // Claude rejects empty text blocks; pad with a single space so the
    // block stays valid and the cache breakpoint lands in place.
    if text.is_empty() {
        text.push(' ');
    }
    if *remaining_slots > 0 && !block_map.contains_key("cache_control") {
        block_map.insert("cache_control".to_string(), cache_control_ephemeral(ttl));
        *remaining_slots = remaining_slots.saturating_sub(1);
    }
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
                .filter(|item| item.contains_key("cache_control"))
                .count();
        }
        Some(Value::Object(item)) if item.contains_key("cache_control") => {
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
                        .filter(|item| item.contains_key("cache_control"))
                        .count();
                }
                Value::Object(item) if item.contains_key("cache_control") => {
                    count += 1;
                }
                _ => {}
            }
        }
    }

    count
}
