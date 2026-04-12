use gproxy_protocol::kinds::ProtocolKind;

use crate::engine::Usage;

/// Extract usage from a non-streaming response body based on the upstream protocol.
pub fn extract_usage(protocol: ProtocolKind, body: &[u8]) -> Option<Usage> {
    match protocol {
        ProtocolKind::OpenAiResponse
        | ProtocolKind::OpenAiChatCompletion
        | ProtocolKind::OpenAi => extract_openai_usage(body),
        ProtocolKind::Claude => extract_claude_usage(body),
        ProtocolKind::Gemini => extract_gemini_usage(body),
        _ => None,
    }
}

/// Extract usage from a single streaming event/chunk.
/// Call this on each chunk; the last non-None result is the final usage.
pub fn extract_stream_usage(protocol: ProtocolKind, chunk: &[u8]) -> Option<Usage> {
    match protocol {
        ProtocolKind::OpenAiChatCompletion => extract_openai_chunk_usage(chunk),
        ProtocolKind::OpenAiResponse | ProtocolKind::OpenAi => {
            extract_openai_response_event_usage(chunk)
        }
        ProtocolKind::Claude => extract_claude_event_usage(chunk),
        ProtocolKind::Gemini => extract_gemini_usage(chunk),
        _ => None,
    }
}

// === Non-streaming extractors ===

fn extract_openai_usage(body: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = v.get("usage")?;
    Some(Usage {
        input_tokens: usage.get("prompt_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("completion_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        tool_uses: Default::default(),
    })
}

fn extract_claude_usage(body: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = v.get("usage")?;
    Some(Usage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_5min: usage
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_5m_input_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_1h: usage
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(|v| v.as_i64()),
        tool_uses: claude_tool_uses(usage),
    })
}

fn extract_gemini_usage(body: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = v.get("usageMetadata")?;
    Some(Usage {
        input_tokens: usage.get("promptTokenCount").and_then(|v| v.as_i64()),
        output_tokens: usage.get("candidatesTokenCount").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("cachedContentTokenCount")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        tool_uses: Default::default(),
    })
}

// === Stream event extractors ===

/// OpenAI ChatCompletions: usage in last chunk's `usage` field
fn extract_openai_chunk_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    let usage = v.get("usage")?;
    if usage.is_null() {
        return None;
    }
    Some(Usage {
        input_tokens: usage.get("prompt_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("completion_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        tool_uses: Default::default(),
    })
}

/// OpenAI Responses: usage in `response.completed` event's `response.usage`
fn extract_openai_response_event_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    if v.get("type")?.as_str()? != "response.completed" {
        return None;
    }
    let usage = v.get("response")?.get("usage")?;
    Some(Usage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        tool_uses: Default::default(),
    })
}

/// Claude: usage in `message_delta` event's `usage` field
fn extract_claude_event_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    if v.get("type")?.as_str()? != "message_delta" {
        return None;
    }
    let usage = v.get("usage")?;
    Some(Usage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_5min: usage
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_5m_input_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_1h: usage
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(|v| v.as_i64()),
        tool_uses: claude_tool_uses(usage),
    })
}

/// Parse Claude's `usage.server_tool_use` block into a `tool_uses` map.
/// Anthropic currently exposes `web_search_requests`; other tools land here
/// as they are released.
fn claude_tool_uses(usage: &serde_json::Value) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    let Some(server_tool_use) = usage.get("server_tool_use") else {
        return out;
    };
    if let Some(n) = server_tool_use
        .get("web_search_requests")
        .and_then(|v| v.as_i64())
    {
        if n > 0 {
            out.insert("web_search".to_string(), n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_usage_extracts_web_search_count() {
        let body = br#"{
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "server_tool_use": { "web_search_requests": 3 }
            }
        }"#;
        let u = extract_claude_usage(body).unwrap();
        assert_eq!(u.input_tokens, Some(100));
        assert_eq!(u.output_tokens, Some(50));
        assert_eq!(u.tool_uses.get("web_search").copied(), Some(3));
    }

    #[test]
    fn claude_event_usage_extracts_web_search_count() {
        let chunk = br#"{
            "type": "message_delta",
            "usage": {
                "input_tokens": 0,
                "output_tokens": 12,
                "server_tool_use": { "web_search_requests": 2 }
            }
        }"#;
        let u = extract_claude_event_usage(chunk).unwrap();
        assert_eq!(u.tool_uses.get("web_search").copied(), Some(2));
    }

    #[test]
    fn claude_usage_without_server_tool_use_has_empty_tool_uses() {
        let body = br#"{
            "usage": { "input_tokens": 10, "output_tokens": 5 }
        }"#;
        let u = extract_claude_usage(body).unwrap();
        assert!(u.tool_uses.is_empty());
    }
}
