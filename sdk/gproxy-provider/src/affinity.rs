use std::collections::HashSet;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::request::PreparedRequest;

pub const DEFAULT_CACHE_AFFINITY_MAX_KEYS: usize = 4096;
const DEFAULT_CACHE_AFFINITY_TTL_MS: u64 = 5 * 60 * 1000;
const ONE_HOUR_CACHE_AFFINITY_TTL_MS: u64 = 60 * 60 * 1000;
const OPENAI_24H_CACHE_AFFINITY_TTL_MS: u64 = 24 * 60 * 60 * 1000;
const GEMINI_CACHED_CONTENT_TTL_MS: u64 = 60 * 60 * 1000;
const NON_CLAUDE_CANDIDATE_LIMIT: usize = 64;
const NON_CLAUDE_CANDIDATE_HEAD: usize = 8;
const NON_CLAUDE_CANDIDATE_TAIL: usize = 56;
const CLAUDE_BREAKPOINT_LOOKBACK: usize = 20;

#[derive(Debug, Clone)]
pub struct CacheAffinityCandidate {
    pub key: String,
    pub ttl_ms: u64,
    pub key_len: usize,
}

#[derive(Debug, Clone)]
pub struct CacheAffinityHint {
    pub candidates: Vec<CacheAffinityCandidate>,
    pub bind: CacheAffinityCandidate,
}

#[derive(Debug, Clone)]
struct CacheAffinityRecord {
    credential_index: usize,
    expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct CacheAffinityPool {
    entries: DashMap<String, CacheAffinityRecord>,
    max_keys: usize,
}

#[derive(Debug, Clone)]
struct ClaudeCacheBlock {
    hash_value: Value,
    explicit_ttl_ms: Option<u64>,
    cacheable: bool,
}

#[derive(Debug, Clone)]
struct ClaudeBreakpoint {
    index: usize,
    ttl_ms: u64,
    kind: &'static str,
}

impl CacheAffinityPool {
    pub fn new(max_keys: usize) -> Self {
        Self {
            entries: DashMap::new(),
            max_keys: max_keys.max(1),
        }
    }

    pub fn get(&self, key: &str) -> Option<usize> {
        let record = self.entries.get(key)?;
        if record.expires_at <= Instant::now() {
            drop(record);
            self.entries.remove(key);
            return None;
        }
        Some(record.credential_index)
    }

    pub fn bind(&self, key: &str, credential_index: usize, ttl_ms: u64) {
        if self.entries.get(key).is_none() {
            self.evict_before_insert(key);
        }
        self.entries.insert(
            key.to_string(),
            CacheAffinityRecord {
                credential_index,
                expires_at: Instant::now() + Duration::from_millis(ttl_ms.max(1)),
            },
        );
    }

    pub fn clear(&self, key: &str) {
        self.entries.remove(key);
    }

    fn evict_before_insert(&self, incoming_key: &str) {
        let now = Instant::now();
        let mut expired_keys = Vec::new();
        let mut live_keys = Vec::new();

        for entry in self.entries.iter() {
            if entry.expires_at <= now {
                expired_keys.push(entry.key().clone());
            } else {
                live_keys.push((entry.key().clone(), entry.expires_at));
            }
        }

        for key in expired_keys {
            self.entries.remove(&key);
        }

        let overflow = live_keys
            .len()
            .saturating_add(1)
            .saturating_sub(self.max_keys);
        if overflow == 0 {
            return;
        }

        live_keys.sort_unstable_by(|(left_key, left_exp), (right_key, right_exp)| {
            left_exp
                .cmp(right_exp)
                .then_with(|| left_key.cmp(right_key))
        });

        for (key, _) in live_keys
            .into_iter()
            .filter(|(key, _)| key != incoming_key)
            .take(overflow)
        {
            self.entries.remove(&key);
        }
    }
}

pub fn cache_affinity_hint_for_request(
    protocol: ProtocolKind,
    request: &PreparedRequest,
) -> Option<CacheAffinityHint> {
    match request.route.operation {
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {}
        _ => return None,
    }

    let body_json = serde_json::from_slice::<Value>(&request.body).ok()?;
    match protocol {
        ProtocolKind::OpenAiResponse => cache_affinity_hint_for_openai_responses(
            request.model.as_deref().unwrap_or("unknown"),
            body_json,
        ),
        ProtocolKind::OpenAiChatCompletion => cache_affinity_hint_for_openai_chat(
            request.model.as_deref().unwrap_or("unknown"),
            body_json,
        ),
        ProtocolKind::Claude => cache_affinity_hint_for_claude_effective_body(body_json),
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            cache_affinity_hint_for_gemini(request.model.as_deref().unwrap_or("unknown"), body_json)
        }
        _ => None,
    }
}

fn cache_affinity_hint_for_claude_effective_body(body_json: Value) -> Option<CacheAffinityHint> {
    cache_affinity_hint_for_claude(body_json, DEFAULT_CACHE_AFFINITY_TTL_MS)
}

fn cache_affinity_hint_for_openai_responses(
    model: &str,
    body_json: Value,
) -> Option<CacheAffinityHint> {
    let ttl_ms = openai_prompt_cache_ttl_ms(body_json.get("prompt_cache_retention"));
    let retention = openai_retention_tag(body_json.get("prompt_cache_retention"));
    let prompt_cache_key_hash = body_json
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(hash_str_to_hex)
        .unwrap_or_else(|| "none".to_string());

    let blocks = openai_responses_cache_blocks(&body_json);
    non_claude_affinity_hint("openai.responses", model, ttl_ms, blocks, |prefix_hash| {
        format!("openai.responses:ret={retention}:k={prompt_cache_key_hash}:h={prefix_hash}")
    })
}

fn cache_affinity_hint_for_openai_chat(model: &str, body_json: Value) -> Option<CacheAffinityHint> {
    let ttl_ms = openai_prompt_cache_ttl_ms(body_json.get("prompt_cache_retention"));
    let retention = openai_retention_tag(body_json.get("prompt_cache_retention"));
    let prompt_cache_key_hash = body_json
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(hash_str_to_hex)
        .unwrap_or_else(|| "none".to_string());

    let blocks = openai_chat_cache_blocks(&body_json);
    non_claude_affinity_hint("openai.chat", model, ttl_ms, blocks, |prefix_hash| {
        format!("openai.chat:ret={retention}:k={prompt_cache_key_hash}:h={prefix_hash}")
    })
}

fn cache_affinity_hint_for_claude(
    body_json: Value,
    default_ttl_ms: u64,
) -> Option<CacheAffinityHint> {
    let blocks = claude_cache_blocks(&body_json, default_ttl_ms);
    if blocks.is_empty() {
        return None;
    }

    let hashes = build_prefix_hashes(
        "claude.messages",
        &blocks
            .iter()
            .map(|block| block.hash_value.clone())
            .collect::<Vec<_>>(),
    )?;
    if hashes.is_empty() {
        return None;
    }

    let mut breakpoints = claude_breakpoints(&body_json, &blocks, default_ttl_ms);
    if breakpoints.is_empty() {
        return None;
    }

    breakpoints.sort_by(|left, right| {
        right
            .index
            .cmp(&left.index)
            .then_with(|| left.kind.cmp(right.kind))
    });

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for breakpoint in breakpoints {
        let start = breakpoint
            .index
            .saturating_sub(CLAUDE_BREAKPOINT_LOOKBACK.saturating_sub(1));
        for idx in (start..=breakpoint.index).rev() {
            let Some(prefix_hash) = hashes.get(idx) else {
                continue;
            };
            let key = format!(
                "claude.messages:ttl={}:bp={}:h={prefix_hash}",
                ttl_tag(breakpoint.ttl_ms),
                breakpoint.kind
            );
            if seen.insert(key.clone()) {
                candidates.push(CacheAffinityCandidate {
                    key_len: key.len(),
                    key,
                    ttl_ms: breakpoint.ttl_ms,
                });
            }
        }
    }

    let bind = candidates.first()?.clone();
    Some(CacheAffinityHint { candidates, bind })
}

fn cache_affinity_hint_for_gemini(model: &str, body_json: Value) -> Option<CacheAffinityHint> {
    if let Some(cached_content) = body_json
        .get("cachedContent")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let key = format!("gemini.cachedContent:{}", hash_str_to_hex(cached_content));
        let candidate = CacheAffinityCandidate {
            key_len: key.len(),
            key,
            ttl_ms: GEMINI_CACHED_CONTENT_TTL_MS,
        };
        return Some(CacheAffinityHint {
            candidates: vec![candidate.clone()],
            bind: candidate,
        });
    }

    let blocks = gemini_cache_blocks(&body_json);
    non_claude_affinity_hint(
        "gemini.generateContent",
        model,
        DEFAULT_CACHE_AFFINITY_TTL_MS,
        blocks,
        |prefix_hash| format!("gemini.generateContent:prefix:{prefix_hash}"),
    )
}

fn non_claude_affinity_hint<F>(
    seed: &str,
    model: &str,
    ttl_ms: u64,
    blocks: Vec<Value>,
    key_builder: F,
) -> Option<CacheAffinityHint>
where
    F: Fn(&str) -> String,
{
    if blocks.is_empty() {
        return None;
    }

    let hash_seed = format!("{seed}:{model}");
    let prefix_hashes = build_prefix_hashes(&hash_seed, &blocks)?;
    let bind_hash = prefix_hashes.last()?;

    let mut candidates = Vec::new();
    for idx in non_claude_candidate_indices(prefix_hashes.len()) {
        let Some(prefix_hash) = prefix_hashes.get(idx) else {
            continue;
        };
        let key = key_builder(prefix_hash);
        candidates.push(CacheAffinityCandidate {
            key_len: key.len(),
            key,
            ttl_ms,
        });
    }

    if candidates.is_empty() {
        return None;
    }

    let bind_key = key_builder(bind_hash);
    let bind = CacheAffinityCandidate {
        key_len: bind_key.len(),
        key: bind_key,
        ttl_ms,
    };

    Some(CacheAffinityHint { candidates, bind })
}

fn openai_chat_cache_blocks(body_json: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();

    if let Some(tools) = body_json.get("tools").and_then(Value::as_array) {
        for (idx, tool) in tools.iter().enumerate() {
            blocks.push(json!({
                "kind": "tools",
                "index": idx,
                "value": tool,
            }));
        }
    }

    if let Some(json_schema) = body_json
        .get("response_format")
        .and_then(|value| value.get("json_schema"))
    {
        blocks.push(json!({
            "kind": "response_format_json_schema",
            "value": json_schema,
        }));
    }

    if let Some(messages) = body_json.get("messages").and_then(Value::as_array) {
        for (message_index, message) in messages.iter().enumerate() {
            push_content_blocks(&mut blocks, "messages", message_index, message, "content");
        }
    }

    blocks
}

fn openai_responses_cache_blocks(body_json: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();

    if let Some(tools) = body_json.get("tools").and_then(Value::as_array) {
        for (idx, tool) in tools.iter().enumerate() {
            blocks.push(json!({
                "kind": "tools",
                "index": idx,
                "value": tool,
            }));
        }
    }

    if let Some(prompt) = body_json.get("prompt").and_then(Value::as_object) {
        let mut prompt_value = serde_json::Map::new();
        if let Some(id) = prompt.get("id") {
            prompt_value.insert("id".to_string(), id.clone());
        }
        if let Some(version) = prompt.get("version") {
            prompt_value.insert("version".to_string(), version.clone());
        }
        if let Some(variables) = prompt.get("variables") {
            prompt_value.insert("variables".to_string(), variables.clone());
        }
        if !prompt_value.is_empty() {
            blocks.push(json!({
                "kind": "prompt",
                "value": Value::Object(prompt_value),
            }));
        }
    }

    if let Some(instructions) = body_json.get("instructions") {
        blocks.push(json!({
            "kind": "instructions",
            "value": instructions,
        }));
    }

    if let Some(input) = body_json.get("input") {
        match input {
            Value::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    push_content_blocks(&mut blocks, "input", idx, item, "content");
                }
            }
            _ => {
                blocks.push(json!({
                    "kind": "input",
                    "index": 0,
                    "value": input,
                }));
            }
        }
    }

    blocks
}

fn gemini_cache_blocks(body_json: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();

    if let Some(system_instruction) = body_json.get("systemInstruction") {
        blocks.push(json!({
            "kind": "system_instruction",
            "value": system_instruction,
        }));
    }

    if let Some(tools) = body_json.get("tools").and_then(Value::as_array) {
        for (idx, tool) in tools.iter().enumerate() {
            blocks.push(json!({
                "kind": "tools",
                "index": idx,
                "value": tool,
            }));
        }
    }

    if let Some(tool_config) = body_json.get("toolConfig") {
        blocks.push(json!({
            "kind": "tool_config",
            "value": tool_config,
        }));
    }

    if let Some(contents) = body_json.get("contents").and_then(Value::as_array) {
        for (content_index, content) in contents.iter().enumerate() {
            push_content_blocks(&mut blocks, "contents", content_index, content, "parts");
        }
    }

    blocks
}

fn claude_cache_blocks(body_json: &Value, default_ttl_ms: u64) -> Vec<ClaudeCacheBlock> {
    let mut blocks = Vec::new();

    if let Some(tools) = body_json.get("tools").and_then(Value::as_array) {
        for (tool_index, tool) in tools.iter().enumerate() {
            let explicit_ttl_ms = tool
                .get("cache_control")
                .map(|value| claude_cache_control_ttl_ms_from_value(value, default_ttl_ms));
            blocks.push(ClaudeCacheBlock {
                hash_value: json!({
                    "section": "tools",
                    "index": tool_index,
                    "value": tool,
                }),
                explicit_ttl_ms,
                cacheable: claude_block_is_cacheable(tool),
            });
        }
    }

    if let Some(system) = body_json.get("system") {
        match system {
            Value::String(text) => {
                let raw = json!({ "type": "text", "text": text });
                blocks.push(ClaudeCacheBlock {
                    hash_value: json!({
                        "section": "system",
                        "index": 0,
                        "value": raw,
                    }),
                    explicit_ttl_ms: None,
                    cacheable: claude_block_is_cacheable(&raw),
                });
            }
            Value::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    let explicit_ttl_ms = item
                        .get("cache_control")
                        .map(|value| claude_cache_control_ttl_ms_from_value(value, default_ttl_ms));
                    blocks.push(ClaudeCacheBlock {
                        hash_value: json!({
                            "section": "system",
                            "index": idx,
                            "value": item,
                        }),
                        explicit_ttl_ms,
                        cacheable: claude_block_is_cacheable(item),
                    });
                }
            }
            _ => {}
        }
    }

    if let Some(messages) = body_json.get("messages").and_then(Value::as_array) {
        for (message_index, message) in messages.iter().enumerate() {
            let role = message.get("role").cloned().unwrap_or(Value::Null);
            match message.get("content") {
                Some(Value::String(text)) => {
                    let raw = json!({ "type": "text", "text": text });
                    blocks.push(ClaudeCacheBlock {
                        hash_value: json!({
                            "section": "messages",
                            "message_index": message_index,
                            "role": role,
                            "content_index": 0,
                            "value": raw,
                        }),
                        explicit_ttl_ms: None,
                        cacheable: claude_block_is_cacheable(&raw),
                    });
                }
                Some(Value::Array(items)) => {
                    for (content_index, item) in items.iter().enumerate() {
                        let explicit_ttl_ms = item.get("cache_control").map(|value| {
                            claude_cache_control_ttl_ms_from_value(value, default_ttl_ms)
                        });
                        blocks.push(ClaudeCacheBlock {
                            hash_value: json!({
                                "section": "messages",
                                "message_index": message_index,
                                "role": role,
                                "content_index": content_index,
                                "value": item,
                            }),
                            explicit_ttl_ms,
                            cacheable: claude_block_is_cacheable(item),
                        });
                    }
                }
                Some(other) => {
                    blocks.push(ClaudeCacheBlock {
                        hash_value: json!({
                            "section": "messages",
                            "message_index": message_index,
                            "role": role,
                            "content_index": 0,
                            "value": other,
                        }),
                        explicit_ttl_ms: None,
                        cacheable: claude_block_is_cacheable(other),
                    });
                }
                None => {}
            }
        }
    }

    blocks
}

fn claude_breakpoints(
    body_json: &Value,
    blocks: &[ClaudeCacheBlock],
    default_ttl_ms: u64,
) -> Vec<ClaudeBreakpoint> {
    let mut breakpoints = Vec::new();

    for (idx, block) in blocks.iter().enumerate() {
        if let Some(ttl_ms) = block.explicit_ttl_ms {
            breakpoints.push(ClaudeBreakpoint {
                index: idx,
                ttl_ms,
                kind: "explicit",
            });
        }
    }

    if let Some(cache_control) = body_json.get("cache_control") {
        let ttl_ms = claude_auto_cache_control_ttl_ms_from_value(cache_control, default_ttl_ms);
        if let Some(index) = blocks.iter().rposition(|block| block.cacheable) {
            breakpoints.push(ClaudeBreakpoint {
                index,
                ttl_ms,
                kind: "auto",
            });
        }
    }

    breakpoints
}

fn claude_block_is_cacheable(block: &Value) -> bool {
    match block {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Object(map) => {
            if let Some(type_name) = map.get("type").and_then(Value::as_str) {
                if matches!(type_name, "thinking" | "redacted_thinking") {
                    return false;
                }
                if type_name == "text"
                    && map
                        .get("text")
                        .and_then(Value::as_str)
                        .is_some_and(|text| text.trim().is_empty())
                {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

fn push_content_blocks(
    blocks: &mut Vec<Value>,
    kind: &str,
    index: usize,
    message: &Value,
    content_field: &str,
) {
    let Some(message_map) = message.as_object() else {
        blocks.push(json!({
            "kind": kind,
            "index": index,
            "value": message,
        }));
        return;
    };

    let mut meta = serde_json::Map::new();
    for (key, value) in message_map {
        if key != content_field {
            meta.insert(key.clone(), value.clone());
        }
    }

    match message_map.get(content_field) {
        Some(Value::Array(parts)) => {
            for (part_index, part) in parts.iter().enumerate() {
                blocks.push(json!({
                    "kind": kind,
                    "index": index,
                    "meta": Value::Object(meta.clone()),
                    "part_index": part_index,
                    "part": part,
                }));
            }
        }
        Some(part) => {
            blocks.push(json!({
                "kind": kind,
                "index": index,
                "meta": Value::Object(meta),
                "part_index": 0,
                "part": part,
            }));
        }
        None => {
            blocks.push(json!({
                "kind": kind,
                "index": index,
                "meta": Value::Object(meta),
            }));
        }
    }
}

fn build_prefix_hashes(seed: &str, blocks: &[Value]) -> Option<Vec<String>> {
    let mut output = Vec::with_capacity(blocks.len());
    for block in blocks {
        let canonical = canonicalize_value(block);
        let bytes = serde_json::to_vec(&canonical).ok()?;
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        output.push(bytes_to_hex(&hasher.finalize()));
    }
    Some(output)
}

fn canonicalize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            let mut out = serde_json::Map::new();
            for (key, value) in entries {
                let canonical = canonicalize_value(value);
                if !canonical.is_null() {
                    out.insert(key.clone(), canonical);
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_value).collect()),
        _ => value.clone(),
    }
}

fn non_claude_candidate_indices(prefix_count: usize) -> Vec<usize> {
    if prefix_count == 0 {
        return Vec::new();
    }

    let mut indices = Vec::new();
    if prefix_count <= NON_CLAUDE_CANDIDATE_LIMIT {
        indices.extend(0..prefix_count);
    } else {
        indices.extend(0..NON_CLAUDE_CANDIDATE_HEAD);
        indices.extend(prefix_count.saturating_sub(NON_CLAUDE_CANDIDATE_TAIL)..prefix_count);
    }

    indices.sort_unstable();
    indices.dedup();
    indices.reverse();
    indices
}

fn ttl_tag(ttl_ms: u64) -> &'static str {
    if ttl_ms == ONE_HOUR_CACHE_AFFINITY_TTL_MS {
        "1h"
    } else {
        "5m"
    }
}

fn claude_cache_control_ttl_ms_from_value(value: &Value, default_ttl_ms: u64) -> u64 {
    match value.get("ttl").and_then(Value::as_str) {
        Some("5m") => DEFAULT_CACHE_AFFINITY_TTL_MS,
        Some("1h") => ONE_HOUR_CACHE_AFFINITY_TTL_MS,
        _ => default_ttl_ms,
    }
}

fn claude_auto_cache_control_ttl_ms_from_value(value: &Value, default_ttl_ms: u64) -> u64 {
    claude_cache_control_ttl_ms_from_value(value, default_ttl_ms)
}

fn openai_retention_tag(prompt_cache_retention: Option<&Value>) -> &'static str {
    if prompt_cache_retention
        .and_then(Value::as_str)
        .is_some_and(|value| value == "24h")
    {
        "24h"
    } else {
        "in-memory"
    }
}

fn openai_prompt_cache_ttl_ms(prompt_cache_retention: Option<&Value>) -> u64 {
    if prompt_cache_retention
        .and_then(Value::as_str)
        .is_some_and(|value| value == "24h")
    {
        OPENAI_24H_CACHE_AFFINITY_TTL_MS
    } else {
        DEFAULT_CACHE_AFFINITY_TTL_MS
    }
}

fn hash_str_to_hex(value: &str) -> String {
    bytes_to_hex(&Sha256::digest(value.as_bytes()))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
