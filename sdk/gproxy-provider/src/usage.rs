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
    // Body shape sniff: Responses API has `output[]`, ChatCompletions has
    // `choices[]`. Each has its own server-side tool signal.
    let tool_uses = if let Some(output) = v.get("output").and_then(|o| o.as_array()) {
        openai_response_output_tool_uses(output)
    } else if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
        chatcompletion_annotations_tool_uses(choices)
    } else {
        Default::default()
    };
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
        tool_uses,
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
    let tool_uses = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .map(|arr| gemini_candidates_tool_uses(arr))
        .unwrap_or_default();
    Some(Usage {
        input_tokens: usage.get("promptTokenCount").and_then(|v| v.as_i64()),
        output_tokens: usage.get("candidatesTokenCount").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("cachedContentTokenCount")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
        tool_uses,
    })
}

// === Stream event extractors ===

/// OpenAI ChatCompletions: usage in last chunk's `usage` field.
/// Server-side tool invocations are detected via
/// `choices[0].message.annotations` / `choices[0].delta.annotations` when
/// present on the final chunk — see
/// [`chatcompletion_annotations_tool_uses`] for the semantics.
fn extract_openai_chunk_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    let usage = v.get("usage")?;
    if usage.is_null() {
        return None;
    }
    let tool_uses = v
        .get("choices")
        .and_then(|c| c.as_array())
        .map(|choices| chatcompletion_annotations_tool_uses(choices))
        .unwrap_or_default();
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
        tool_uses,
    })
}

/// OpenAI Responses: handles two event types.
///
/// - `response.completed` → extracts the final token usage from
///   `response.usage`. `response.output[]` is **empty** in the completed
///   snapshot (per the OpenAI Responses streaming spec and confirmed by
///   `transform::stream_to_nonstream::response`), so no tool counts here.
/// - `response.output_item.done` → emits one tool invocation per item
///   that matches a server-side tool type. The caller's `merge_usage`
///   must additively sum `tool_uses` across chunks so these per-item
///   emits accumulate into the final count.
fn extract_openai_response_event_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    match v.get("type")?.as_str()? {
        "response.completed" => {
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
        "response.output_item.done" => {
            let item = v.get("item")?;
            let tool_key = openai_response_item_tool_key(item)?;
            let mut tool_uses = std::collections::BTreeMap::new();
            tool_uses.insert(tool_key.to_string(), 1);
            Some(Usage {
                input_tokens: None,
                output_tokens: None,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
                tool_uses,
            })
        }
        _ => None,
    }
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

// === Per-protocol tool_uses helpers ===

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
        && n > 0
    {
        out.insert("web_search".to_string(), n);
    }
    out
}

/// Map an OpenAI Responses `output[]` item's `type` string to the tool key
/// used in `ModelPrice.tool_call_prices`. Returns `None` for non-tool items
/// (messages, reasoning, etc.) and for client-side function calls.
fn openai_response_item_tool_key(item: &serde_json::Value) -> Option<&'static str> {
    match item.get("type")?.as_str()? {
        "web_search_call" => Some("web_search"),
        "file_search_call" => Some("file_search"),
        "code_interpreter_call" => Some("code_interpreter"),
        _ => None,
    }
}

/// Count server-side tool invocations in an OpenAI Responses non-streaming
/// `output[]` array. Each output item is one invocation.
fn openai_response_output_tool_uses(
    output: &[serde_json::Value],
) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    for item in output {
        if let Some(tool_key) = openai_response_item_tool_key(item) {
            *out.entry(tool_key.to_string()).or_insert(0) += 1;
        }
    }
    out
}

/// Count server-side tool invocations from ChatCompletions
/// `choices[0].message.annotations` (or `delta.annotations` on streaming).
///
/// **Semantics:** ChatCompletions does not expose an accurate per-query
/// count for `web_search_preview`. The only signal is `url_citation`
/// annotations in the assistant message, which are **per-URL** — a single
/// search query can produce multiple URL citations. We conservatively emit
/// `{ "web_search": 1 }` if any `url_citation` is present, undercounting
/// the multi-query case but never overcharging. Users who need precise
/// tool billing should use the Responses API, which reports each tool
/// invocation as a separate `output[]` item.
fn chatcompletion_annotations_tool_uses(
    choices: &[serde_json::Value],
) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    let mut saw_url_citation = false;
    for choice in choices {
        // Non-streaming has `message.annotations`, streaming has `delta.annotations`.
        for source_key in ["message", "delta"] {
            let Some(annotations) = choice
                .get(source_key)
                .and_then(|m| m.get("annotations"))
                .and_then(|a| a.as_array())
            else {
                continue;
            };
            for ann in annotations {
                if ann.get("type").and_then(|t| t.as_str()) == Some("url_citation") {
                    saw_url_citation = true;
                }
            }
        }
    }
    if saw_url_citation {
        out.insert("web_search".to_string(), 1);
    }
    out
}

/// Count server-side tool invocations from Gemini
/// `candidates[i]` entries. Counts:
///
/// - `groundingMetadata.webSearchQueries[]` length → `google_search`
/// - `urlContextMetadata.urlMetadata[]` length → `url_context`
/// - `content.parts[]` items containing `executableCode` → `code_execution`
///
/// **Streaming caveat:** the same extractor is used for both streaming and
/// non-streaming paths. Gemini's grounding metadata typically appears only
/// on the final chunk with the complete `candidates[]`, so per-chunk calls
/// emit empty counts until the final chunk. If a future Gemini stream
/// shape emits partial grounding metadata on multiple chunks, the caller's
/// additive merge would overcount — this is a best-effort signal.
fn gemini_candidates_tool_uses(
    candidates: &[serde_json::Value],
) -> std::collections::BTreeMap<String, i64> {
    let mut out = std::collections::BTreeMap::new();
    for candidate in candidates {
        if let Some(queries) = candidate
            .get("groundingMetadata")
            .and_then(|g| g.get("webSearchQueries"))
            .and_then(|q| q.as_array())
            && !queries.is_empty()
        {
            *out.entry("google_search".to_string()).or_insert(0) += queries.len() as i64;
        }
        if let Some(urls) = candidate
            .get("urlContextMetadata")
            .and_then(|u| u.get("urlMetadata"))
            .and_then(|u| u.as_array())
            && !urls.is_empty()
        {
            *out.entry("url_context".to_string()).or_insert(0) += urls.len() as i64;
        }
        if let Some(parts) = candidate
            .get("content")
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
        {
            let code_count = parts
                .iter()
                .filter(|p| p.get("executableCode").is_some())
                .count();
            if code_count > 0 {
                *out.entry("code_execution".to_string()).or_insert(0) += code_count as i64;
            }
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

    // === OpenAI Responses (non-streaming) ===

    #[test]
    fn openai_responses_non_streaming_counts_output_tool_items() {
        let body = br#"{
            "usage": { "prompt_tokens": 100, "completion_tokens": 50 },
            "output": [
                { "type": "web_search_call", "id": "ws_1" },
                { "type": "file_search_call", "id": "fs_1" },
                { "type": "web_search_call", "id": "ws_2" },
                { "type": "message", "content": [] },
                { "type": "function_call", "name": "user_fn" }
            ]
        }"#;
        let u = extract_openai_usage(body).unwrap();
        assert_eq!(u.input_tokens, Some(100));
        assert_eq!(u.output_tokens, Some(50));
        assert_eq!(u.tool_uses.get("web_search").copied(), Some(2));
        assert_eq!(u.tool_uses.get("file_search").copied(), Some(1));
        // message and function_call are not billed server-side tools.
        assert!(!u.tool_uses.contains_key("function_call"));
    }

    // === OpenAI Responses (streaming) ===

    #[test]
    fn openai_responses_stream_output_item_done_emits_single_count() {
        let chunk = br#"{
            "type": "response.output_item.done",
            "output_index": 0,
            "sequence_number": 5,
            "item": { "type": "web_search_call", "id": "ws_1" }
        }"#;
        let u = extract_openai_response_event_usage(chunk).unwrap();
        assert_eq!(u.tool_uses.get("web_search").copied(), Some(1));
        assert!(u.input_tokens.is_none());
        assert!(u.output_tokens.is_none());
    }

    #[test]
    fn openai_responses_stream_completed_has_tokens_no_tool_uses() {
        let chunk = br#"{
            "type": "response.completed",
            "sequence_number": 42,
            "response": {
                "usage": { "input_tokens": 100, "output_tokens": 50 }
            }
        }"#;
        let u = extract_openai_response_event_usage(chunk).unwrap();
        assert_eq!(u.input_tokens, Some(100));
        assert_eq!(u.output_tokens, Some(50));
        // output[] is empty in completed events; tool_uses come from the
        // incremental output_item.done events via additive merge.
        assert!(u.tool_uses.is_empty());
    }

    #[test]
    fn openai_responses_stream_non_tool_item_returns_none_or_empty() {
        let chunk = br#"{
            "type": "response.output_item.done",
            "output_index": 1,
            "sequence_number": 6,
            "item": { "type": "message", "content": [] }
        }"#;
        // Non-tool items (messages, reasoning, function_call) yield no Usage.
        assert!(extract_openai_response_event_usage(chunk).is_none());
    }

    // === OpenAI ChatCompletions (non-streaming) ===

    #[test]
    fn openai_chatcompletions_url_citation_counts_one_web_search() {
        // ChatCompletions cannot report precise per-query counts for
        // web_search_preview, so we emit a conservative {web_search: 1}
        // when any url_citation annotation is present, regardless of how
        // many URLs are cited.
        let body = br#"{
            "usage": { "prompt_tokens": 10, "completion_tokens": 20 },
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "...",
                        "annotations": [
                            { "type": "url_citation", "url_citation": { "url": "https://a.com", "title": "A" } },
                            { "type": "url_citation", "url_citation": { "url": "https://b.com", "title": "B" } }
                        ]
                    }
                }
            ]
        }"#;
        let u = extract_openai_usage(body).unwrap();
        assert_eq!(u.tool_uses.get("web_search").copied(), Some(1));
    }

    #[test]
    fn openai_chatcompletions_no_annotations_no_tool_uses() {
        let body = br#"{
            "usage": { "prompt_tokens": 10, "completion_tokens": 20 },
            "choices": [
                { "message": { "role": "assistant", "content": "hi" } }
            ]
        }"#;
        let u = extract_openai_usage(body).unwrap();
        assert!(u.tool_uses.is_empty());
    }

    // === Gemini (non-streaming) ===

    #[test]
    fn gemini_google_search_counts_web_search_queries() {
        let body = br#"{
            "usageMetadata": { "promptTokenCount": 50, "candidatesTokenCount": 30 },
            "candidates": [
                {
                    "content": { "parts": [{ "text": "..." }] },
                    "groundingMetadata": {
                        "webSearchQueries": ["query 1", "query 2", "query 3"]
                    }
                }
            ]
        }"#;
        let u = extract_gemini_usage(body).unwrap();
        assert_eq!(u.tool_uses.get("google_search").copied(), Some(3));
    }

    #[test]
    fn gemini_url_context_counts_url_metadata() {
        let body = br#"{
            "usageMetadata": { "promptTokenCount": 50, "candidatesTokenCount": 30 },
            "candidates": [
                {
                    "content": { "parts": [] },
                    "urlContextMetadata": {
                        "urlMetadata": [
                            { "retrievedUrl": "https://a.com", "urlRetrievalStatus": "URL_RETRIEVAL_STATUS_SUCCESS" },
                            { "retrievedUrl": "https://b.com", "urlRetrievalStatus": "URL_RETRIEVAL_STATUS_SUCCESS" }
                        ]
                    }
                }
            ]
        }"#;
        let u = extract_gemini_usage(body).unwrap();
        assert_eq!(u.tool_uses.get("url_context").copied(), Some(2));
    }

    #[test]
    fn gemini_code_execution_counts_executable_code_parts() {
        let body = br#"{
            "usageMetadata": { "promptTokenCount": 50, "candidatesTokenCount": 30 },
            "candidates": [
                {
                    "content": {
                        "parts": [
                            { "text": "Let me compute." },
                            { "executableCode": { "language": "PYTHON", "code": "print(1+1)" } },
                            { "codeExecutionResult": { "outcome": "OUTCOME_OK", "output": "2" } },
                            { "executableCode": { "language": "PYTHON", "code": "print(2*2)" } }
                        ]
                    }
                }
            ]
        }"#;
        let u = extract_gemini_usage(body).unwrap();
        // Counts only executableCode parts (the invocation), not result parts.
        assert_eq!(u.tool_uses.get("code_execution").copied(), Some(2));
    }

    #[test]
    fn gemini_without_grounding_has_empty_tool_uses() {
        let body = br#"{
            "usageMetadata": { "promptTokenCount": 50, "candidatesTokenCount": 30 },
            "candidates": [{ "content": { "parts": [{ "text": "hi" }] } }]
        }"#;
        let u = extract_gemini_usage(body).unwrap();
        assert!(u.tool_uses.is_empty());
    }
}
