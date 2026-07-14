use serde_json::{Value, json};

use crate::protocol::{claude, gemini, openai};
use crate::transform::TransformError;

const PROMPT_CACHE_KEY_LIMIT: usize = 64;
const PROMPT_CACHE_KEY_DOMAIN: &[u8] = b"gproxy-prompt-cache-key-v1";

#[derive(Clone, Copy)]
enum ClaudeCacheLocation {
    Tool(usize),
    System(usize),
    Message {
        message_index: usize,
        block_index: usize,
    },
}

pub(in crate::transform::generate_content) fn openai_breakpoint(
    cache_control: Option<claude::CacheControl>,
) -> Option<openai::PromptCacheBreakpoint> {
    cache_control.map(|_| openai::PromptCacheBreakpoint {
        mode: openai::PromptCacheBreakpointMode::Explicit,
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn claude_cache_control(
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::CacheControl> {
    breakpoint.map(|_| claude::CacheControl {
        type_: claude::CacheControlType::Ephemeral,
        // OpenAI's request-wide 30m TTL has no exact Claude equivalent. Use
        // Claude's default 5m TTL, as agreed, rather than silently paying for 1h.
        ttl: None,
        extra: Default::default(),
    })
}

pub(crate) fn openai_options_for_claude_root(
    cache_control: Option<claude::CacheControl>,
) -> Option<openai::PromptCacheOptions> {
    cache_control.map(|_| openai::PromptCacheOptions {
        mode: Some(openai::PromptCacheMode::Implicit),
        ttl: Some(openai::PromptCacheTtl::ThirtyMinutes),
        extra: Default::default(),
    })
}

/// Derive the stable routing key OpenAI uses to improve prompt-cache affinity.
///
/// Claude Code embeds its real session id in `metadata.user_id`; use that id
/// directly, matching Codex's session-scoped key. Generic Claude clients do
/// not expose a conversation id, so fall back to a digest of the stable
/// conversation identity (`system` + the first message).
pub(crate) fn claude_prompt_cache_key(input: &claude::CreateMessageRequestBody) -> String {
    if let Some(session_id) = input
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.user_id.as_deref())
        .and_then(claude_session_id)
    {
        return bounded_prompt_cache_key("claude", &session_id);
    }

    let system = input
        .system
        .as_ref()
        .and_then(|system| serde_json::to_vec(system).ok())
        .unwrap_or_default();
    let first_message = input
        .messages
        .first()
        .and_then(|message| serde_json::to_vec(message).ok())
        .unwrap_or_default();
    derived_prompt_cache_key("claude", &system, &first_message)
}

/// Reuse Gemini's explicit cached-content name when present. Otherwise derive
/// a conversation-scoped key from the system instruction and first content so
/// appending later turns does not change cache routing.
pub(in crate::transform::generate_content) fn gemini_prompt_cache_key(
    input: &gemini::GenerateContentRequest,
) -> String {
    if let Some(cached_content) = input
        .cached_content
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return bounded_prompt_cache_key("gemini", cached_content);
    }

    let system = input
        .system_instruction
        .as_ref()
        .and_then(|system| serde_json::to_vec(system).ok())
        .unwrap_or_default();
    let first_message = input
        .contents
        .first()
        .and_then(|message| serde_json::to_vec(message).ok())
        .unwrap_or_default();
    derived_prompt_cache_key("gemini", &system, &first_message)
}

fn claude_session_id(user_id: &str) -> Option<String> {
    let value: Value = serde_json::from_str(user_id).ok()?;
    value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn bounded_prompt_cache_key(namespace: &str, value: &str) -> String {
    if value.len() <= PROMPT_CACHE_KEY_LIMIT {
        value.to_owned()
    } else {
        derived_prompt_cache_key(namespace, value.as_bytes(), &[])
    }
}

fn derived_prompt_cache_key(namespace: &str, system: &[u8], first_message: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(PROMPT_CACHE_KEY_DOMAIN);
    update_hash_component(&mut hasher, namespace.as_bytes());
    update_hash_component(&mut hasher, system);
    update_hash_component(&mut hasher, first_message);
    let digest = hasher.finalize().to_hex();
    format!("gproxy:{namespace}:{}", &digest.as_str()[..32])
}

fn update_hash_component(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

pub(in crate::transform::generate_content) fn openai_cache_mode_is_explicit(
    options: Option<&openai::PromptCacheOptions>,
) -> bool {
    matches!(
        options.and_then(|options| options.mode.as_ref()),
        Some(openai::PromptCacheMode::Explicit)
    )
}

/// Map OpenAI's request-wide cache policy onto Claude's four-slot model.
///
/// Explicit mode keeps the final four explicit block markers. Implicit mode
/// uses one top-level automatic marker and keeps only the final three explicit
/// markers. OpenAI's missing mode is implicit by definition.
pub(in crate::transform::generate_content) fn apply_openai_cache_policy(
    body: claude::CreateMessageRequestBody,
    explicit_mode: bool,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let mut value = serde_json::to_value(body).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })?;
    let root = value
        .as_object_mut()
        .ok_or_else(|| TransformError::Serialization {
            reason: "Claude request did not serialize to an object".to_owned(),
        })?;

    if explicit_mode {
        root.remove("cache_control");
    } else {
        root.insert("cache_control".into(), json!({"type": "ephemeral"}));
    }

    let keep = if explicit_mode { 4 } else { 3 };
    let locations = claude_cache_locations(root);
    let drop_count = locations.len().saturating_sub(keep);
    for location in locations.into_iter().take(drop_count) {
        if let Some(block) = claude_cache_block_mut(root, location) {
            block.remove("cache_control");
        }
    }

    serde_json::from_value(value).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })
}

fn claude_cache_locations(root: &serde_json::Map<String, Value>) -> Vec<ClaudeCacheLocation> {
    let mut locations = Vec::new();
    if let Some(tools) = root.get("tools").and_then(Value::as_array) {
        for (index, tool) in tools.iter().enumerate() {
            if tool.get("cache_control").is_some() {
                locations.push(ClaudeCacheLocation::Tool(index));
            }
        }
    }
    if let Some(system) = root.get("system").and_then(Value::as_array) {
        for (index, block) in system.iter().enumerate() {
            if block.get("cache_control").is_some() {
                locations.push(ClaudeCacheLocation::System(index));
            }
        }
    }
    if let Some(messages) = root.get("messages").and_then(Value::as_array) {
        for (message_index, message) in messages.iter().enumerate() {
            let Some(blocks) = message.get("content").and_then(Value::as_array) else {
                continue;
            };
            for (block_index, block) in blocks.iter().enumerate() {
                if block.get("cache_control").is_some() {
                    locations.push(ClaudeCacheLocation::Message {
                        message_index,
                        block_index,
                    });
                }
            }
        }
    }
    locations
}

fn claude_cache_block_mut(
    root: &mut serde_json::Map<String, Value>,
    location: ClaudeCacheLocation,
) -> Option<&mut serde_json::Map<String, Value>> {
    let block = match location {
        ClaudeCacheLocation::Tool(index) => {
            root.get_mut("tools")?.as_array_mut()?.get_mut(index)?
        }
        ClaudeCacheLocation::System(index) => {
            root.get_mut("system")?.as_array_mut()?.get_mut(index)?
        }
        ClaudeCacheLocation::Message {
            message_index,
            block_index,
        } => root
            .get_mut("messages")?
            .as_array_mut()?
            .get_mut(message_index)?
            .get_mut("content")?
            .as_array_mut()?
            .get_mut(block_index)?,
    };
    block.as_object_mut()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn claude_uses_claude_code_session_id() {
        let input: claude::CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "metadata": {
                "user_id": "{\"device_id\":\"device-1\",\"session_id\":\"session-123\"}"
            },
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .unwrap();

        assert_eq!(claude_prompt_cache_key(&input), "session-123");
    }

    #[test]
    fn claude_fallback_is_stable_when_conversation_grows() {
        let first: claude::CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "system": "stable system",
            "messages": [{"role": "user", "content": "first question"}]
        }))
        .unwrap();
        let grown: claude::CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-opus-4-8",
            "max_tokens": 4096,
            "system": "stable system",
            "messages": [
                {"role": "user", "content": "first question"},
                {"role": "assistant", "content": "first answer"},
                {"role": "user", "content": "follow-up"}
            ]
        }))
        .unwrap();
        let different: claude::CreateMessageRequestBody = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "system": "stable system",
            "messages": [{"role": "user", "content": "different question"}]
        }))
        .unwrap();

        let key = claude_prompt_cache_key(&first);
        assert_eq!(key, claude_prompt_cache_key(&grown));
        assert_ne!(key, claude_prompt_cache_key(&different));
        assert!(key.starts_with("gproxy:claude:"));
        assert!(key.len() <= PROMPT_CACHE_KEY_LIMIT);
    }

    #[test]
    fn gemini_reuses_cached_content_or_stable_fallback() {
        let cached: gemini::GenerateContentRequest = serde_json::from_value(json!({
            "cachedContent": "cachedContents/example",
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}]
        }))
        .unwrap();
        assert_eq!(gemini_prompt_cache_key(&cached), "cachedContents/example");

        let first: gemini::GenerateContentRequest = serde_json::from_value(json!({
            "systemInstruction": {"parts": [{"text": "stable system"}]},
            "contents": [{"role": "user", "parts": [{"text": "first question"}]}]
        }))
        .unwrap();
        let grown: gemini::GenerateContentRequest = serde_json::from_value(json!({
            "model": "gemini-3-pro",
            "systemInstruction": {"parts": [{"text": "stable system"}]},
            "contents": [
                {"role": "user", "parts": [{"text": "first question"}]},
                {"role": "model", "parts": [{"text": "first answer"}]},
                {"role": "user", "parts": [{"text": "follow-up"}]}
            ]
        }))
        .unwrap();

        let key = gemini_prompt_cache_key(&first);
        assert_eq!(key, gemini_prompt_cache_key(&grown));
        assert!(key.starts_with("gproxy:gemini:"));
        assert!(key.len() <= PROMPT_CACHE_KEY_LIMIT);
    }

    #[test]
    fn overlong_explicit_key_is_bounded() {
        let key = bounded_prompt_cache_key("gemini", &"x".repeat(256));
        assert!(key.starts_with("gproxy:gemini:"));
        assert!(key.len() <= PROMPT_CACHE_KEY_LIMIT);
    }
}
