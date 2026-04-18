use gproxy_protocol::kinds::ProtocolKind;

/// Token usage extracted from upstream response.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_creation_input_tokens_5min: Option<i64>,
    pub cache_creation_input_tokens_1h: Option<i64>,
}

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
    })
}

// === Stream event extractors ===

/// OpenAI ChatCompletions: usage in last chunk's `usage` field.
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
    })
}

/// OpenAI Responses: extracts token usage from `response.completed` events.
/// `response.output[]` is empty in the completed snapshot per the OpenAI
/// Responses streaming spec (confirmed by
/// `transform::stream_to_nonstream::response`).
fn extract_openai_response_event_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    if v.get("type")?.as_str()? != "response.completed" {
        return None;
    }
    let usage = v.get("response")?.get("usage")?;
    Some(Usage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("input_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: None,
        cache_creation_input_tokens_5min: None,
        cache_creation_input_tokens_1h: None,
    })
}

/// Claude: usage in `message_delta` event's `usage` field
fn extract_claude_event_usage(chunk: &[u8]) -> Option<Usage> {
    let v: serde_json::Value = serde_json::from_slice(chunk).ok()?;
    if v.get("type")?.as_str()? != "message_delta" {
        return None;
    }
    let usage = v.get("usage")?;
    // `cache_creation` may be directly in `usage` or nested inside `usage.iterations[0]`
    let cache_creation = usage.get("cache_creation").or_else(|| {
        usage
            .get("iterations")
            .and_then(|it| it.get(0))
            .and_then(|first| first.get("cache_creation"))
    });
    Some(Usage {
        input_tokens: usage.get("input_tokens").and_then(|v| v.as_i64()),
        output_tokens: usage.get("output_tokens").and_then(|v| v.as_i64()),
        cache_read_input_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_5min: cache_creation
            .and_then(|c| c.get("ephemeral_5m_input_tokens"))
            .and_then(|v| v.as_i64()),
        cache_creation_input_tokens_1h: cache_creation
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(|v| v.as_i64()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_openai_response_event_usage_reads_cached_tokens() {
        let chunk = serde_json::json!({
            "type": "response.completed",
            "response": {
                "usage": {
                    "input_tokens": 41104,
                    "output_tokens": 476,
                    "input_tokens_details": {
                        "cached_tokens": 32000
                    }
                }
            }
        });

        let usage = extract_stream_usage(
            ProtocolKind::OpenAiResponse,
            serde_json::to_string(&chunk)
                .expect("serialize chunk")
                .as_bytes(),
        )
        .expect("usage");

        assert_eq!(usage.input_tokens, Some(41104));
        assert_eq!(usage.output_tokens, Some(476));
        assert_eq!(usage.cache_read_input_tokens, Some(32000));
    }
}

// ---------------------------------------------------------------------------
// Stream usage observation (pre-transform, upstream-protocol-aware)
// ---------------------------------------------------------------------------

/// Decode raw upstream response bytes into discrete JSON chunks, regardless
/// of transport (SSE for most protocols, NDJSON for Gemini streaming).
pub enum UsageChunkDecoder {
    Sse(gproxy_protocol::stream::SseToNdjsonRewriter),
    Ndjson(Vec<u8>),
}

impl UsageChunkDecoder {
    pub fn new(protocol: ProtocolKind) -> Self {
        match protocol {
            ProtocolKind::Claude
            | ProtocolKind::OpenAi
            | ProtocolKind::OpenAiResponse
            | ProtocolKind::OpenAiChatCompletion
            | ProtocolKind::Gemini => Self::Sse(Default::default()),
            ProtocolKind::GeminiNDJson => Self::Ndjson(Vec::new()),
        }
    }

    pub fn push_chunk(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Sse(rewriter) => split_usage_lines(&rewriter.push_chunk(chunk), &mut out),
            Self::Ndjson(pending) => {
                pending.extend_from_slice(chunk);
                drain_usage_lines(pending, &mut out);
            }
        }
        out
    }

    pub fn finish(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Sse(rewriter) => split_usage_lines(&rewriter.finish(), &mut out),
            Self::Ndjson(pending) => {
                if !pending.is_empty() {
                    let mut line = std::mem::take(pending);
                    if line.last().copied() == Some(b'\r') {
                        line.pop();
                    }
                    if !line.is_empty() {
                        out.push(line);
                    }
                }
            }
        }
        out
    }
}

use gproxy_protocol::stream::{drain_lines as drain_usage_lines, split_lines as split_usage_lines};

/// Extract any assistant-visible text from a single decoded JSON chunk.
/// Used to estimate output tokens when the upstream stream is interrupted
/// before emitting a terminal usage report.
pub fn extract_partial_output_text(protocol: ProtocolKind, json_chunk: &[u8]) -> Option<String> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_protocol::claude::create_message::stream::{
                BetaRawContentBlockDelta, ClaudeStreamEvent,
            };

            let event: ClaudeStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ClaudeStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                    BetaRawContentBlockDelta::Text { text } => Some(text),
                    BetaRawContentBlockDelta::Thinking { thinking } => Some(thinking),
                    BetaRawContentBlockDelta::InputJson { partial_json } => Some(partial_json),
                    BetaRawContentBlockDelta::Compaction { content } => content,
                    BetaRawContentBlockDelta::Citations { .. }
                    | BetaRawContentBlockDelta::Signature { .. } => None,
                },
                _ => None,
            }
        }
        ProtocolKind::OpenAiChatCompletion => {
            use gproxy_protocol::openai::create_chat_completions::stream::ChatCompletionChunk;

            let chunk: ChatCompletionChunk = serde_json::from_slice(json_chunk).ok()?;
            let mut parts = Vec::new();
            for choice in chunk.choices {
                let delta = choice.delta;
                if let Some(text) = delta.content
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(text) = delta.reasoning_content
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(text) = delta.refusal
                    && !text.is_empty()
                {
                    parts.push(text);
                }
                if let Some(function_call) = delta.function_call {
                    if let Some(name) = function_call.name
                        && !name.is_empty()
                    {
                        parts.push(name);
                    }
                    if let Some(arguments) = function_call.arguments
                        && !arguments.is_empty()
                    {
                        parts.push(arguments);
                    }
                }
                if let Some(tool_calls) = delta.tool_calls {
                    for tool_call in tool_calls {
                        if let Some(function) = tool_call.function {
                            if let Some(name) = function.name
                                && !name.is_empty()
                            {
                                parts.push(name);
                            }
                            if let Some(arguments) = function.arguments
                                && !arguments.is_empty()
                            {
                                parts.push(arguments);
                            }
                        }
                    }
                }
            }
            (!parts.is_empty()).then_some(parts.join("\n"))
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_protocol::openai::create_response::stream::ResponseStreamEvent;

            let event: ResponseStreamEvent = serde_json::from_slice(json_chunk).ok()?;
            match event {
                ResponseStreamEvent::OutputTextDelta { delta, .. }
                | ResponseStreamEvent::RefusalDelta { delta, .. }
                | ResponseStreamEvent::ReasoningTextDelta { delta, .. }
                | ResponseStreamEvent::ReasoningSummaryTextDelta { delta, .. }
                | ResponseStreamEvent::FunctionCallArgumentsDelta { delta, .. }
                | ResponseStreamEvent::CustomToolCallInputDelta { delta, .. }
                | ResponseStreamEvent::McpCallArgumentsDelta { delta, .. }
                | ResponseStreamEvent::AudioTranscriptDelta { delta, .. }
                | ResponseStreamEvent::CodeInterpreterCallCodeDelta { delta, .. } => {
                    (!delta.is_empty()).then_some(delta)
                }
                _ => None,
            }
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            use gproxy_protocol::gemini::generate_content::response::ResponseBody;

            let chunk: ResponseBody = serde_json::from_slice(json_chunk).ok()?;
            let mut parts = Vec::new();
            if let Some(candidates) = chunk.candidates {
                for candidate in candidates {
                    if let Some(content) = candidate.content {
                        for part in content.parts {
                            if let Some(text) = part.text
                                && !text.is_empty()
                            {
                                parts.push(text);
                            }
                            if let Some(function_call) = part.function_call {
                                if !function_call.name.is_empty() {
                                    parts.push(function_call.name);
                                }
                                if let Some(args) = function_call.args
                                    && let Ok(json) = serde_json::to_string(&args)
                                    && !json.is_empty()
                                {
                                    parts.push(json);
                                }
                            }
                        }
                    }
                    if let Some(message) = candidate.finish_message
                        && !message.is_empty()
                    {
                        parts.push(message);
                    }
                }
            }
            if let Some(status) = chunk.model_status
                && let Some(message) = status.message
                && !message.is_empty()
            {
                parts.push(message);
            }
            (!parts.is_empty()).then_some(parts.join("\n"))
        }
        _ => None,
    }
}

/// Aggregated state produced by observing an upstream stream. Constructed by
/// the engine in `wrap_upstream_response_stream` against the **upstream**
/// protocol (`dst_proto`). The handler consumes this snapshot after the
/// stream drains (or on drop) to persist usage — values here always reflect
/// the upstream-native fields, immune to downstream cross-protocol
/// transforms.
#[derive(Debug, Default, Clone)]
pub struct StreamUsageSnapshot {
    pub partial_usage: Usage,
    pub last_usage: Option<Usage>,
    pub partial_output: String,
    pub finalized: bool,
}

pub struct StreamUsageObserver {
    protocol: ProtocolKind,
    decoder: UsageChunkDecoder,
    state: std::sync::Arc<std::sync::Mutex<StreamUsageSnapshot>>,
}

impl StreamUsageObserver {
    pub fn new(protocol: ProtocolKind) -> Self {
        Self {
            protocol,
            decoder: UsageChunkDecoder::new(protocol),
            state: std::sync::Arc::new(std::sync::Mutex::new(StreamUsageSnapshot::default())),
        }
    }

    pub fn shared_state(&self) -> std::sync::Arc<std::sync::Mutex<StreamUsageSnapshot>> {
        self.state.clone()
    }

    pub fn protocol(&self) -> ProtocolKind {
        self.protocol
    }

    pub fn observe_chunk(&mut self, chunk: &[u8]) {
        for json_chunk in self.decoder.push_chunk(chunk) {
            self.observe_json(&json_chunk);
        }
    }

    pub fn finish(&mut self) {
        let flushed = self.decoder.finish();
        for json_chunk in flushed {
            self.observe_json(&json_chunk);
        }
    }

    fn observe_json(&self, json_chunk: &[u8]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.finalized {
            return;
        }

        if let Some(usage) = extract_stream_usage(self.protocol, json_chunk) {
            merge_stream_usage(&mut state.partial_usage, &usage);
            state.last_usage = Some(usage);
        } else if let Some(usage) = extract_stream_start_usage(self.protocol, json_chunk) {
            merge_stream_usage(&mut state.partial_usage, &usage);
        }

        if let Some(text) = extract_partial_output_text(self.protocol, json_chunk) {
            state.partial_output.push_str(&text);
        }
    }
}

/// Merge tokens from `src` into `dst`, preserving previously-set fields when
/// `src` omits them. Used to reconcile values emitted across multiple
/// events (e.g. Claude `message_start` + `message_delta`).
pub fn merge_stream_usage(dst: &mut Usage, src: &Usage) {
    if src.input_tokens.is_some() {
        dst.input_tokens = src.input_tokens;
    }
    if src.output_tokens.is_some() {
        dst.output_tokens = src.output_tokens;
    }
    if src.cache_read_input_tokens.is_some() {
        dst.cache_read_input_tokens = src.cache_read_input_tokens;
    }
    if src.cache_creation_input_tokens.is_some() {
        dst.cache_creation_input_tokens = src.cache_creation_input_tokens;
    }
    if src.cache_creation_input_tokens_5min.is_some() {
        dst.cache_creation_input_tokens_5min = src.cache_creation_input_tokens_5min;
    }
    if src.cache_creation_input_tokens_1h.is_some() {
        dst.cache_creation_input_tokens_1h = src.cache_creation_input_tokens_1h;
    }
}

pub fn stream_usage_has_any_value(usage: &Usage) -> bool {
    usage.input_tokens.is_some()
        || usage.output_tokens.is_some()
        || usage.cache_read_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some()
        || usage.cache_creation_input_tokens_5min.is_some()
        || usage.cache_creation_input_tokens_1h.is_some()
}

/// Extract usage from the protocol's **first** stream event (e.g. Claude
/// `message_start`). The `message_start` event carries the authoritative
/// `cache_creation.{ephemeral_5m_input_tokens,ephemeral_1h_input_tokens}`
/// breakdown that `message_delta` omits, so this is the only place the
/// 5m/1h split can be captured for Claude streams.
pub fn extract_stream_start_usage(protocol: ProtocolKind, chunk: &[u8]) -> Option<Usage> {
    match protocol {
        ProtocolKind::Claude => {
            use gproxy_protocol::claude::create_message::stream::ClaudeStreamEvent;

            let event: ClaudeStreamEvent = serde_json::from_slice(chunk).ok()?;
            match event {
                ClaudeStreamEvent::MessageStart { message } => {
                    let usage = &message.usage;
                    Some(Usage {
                        input_tokens: i64::try_from(usage.input_tokens).ok(),
                        output_tokens: i64::try_from(usage.output_tokens).ok(),
                        cache_read_input_tokens: i64::try_from(usage.cache_read_input_tokens).ok(),
                        cache_creation_input_tokens: i64::try_from(
                            usage.cache_creation_input_tokens,
                        )
                        .ok(),
                        cache_creation_input_tokens_5min: i64::try_from(
                            usage.cache_creation.ephemeral_5m_input_tokens,
                        )
                        .ok(),
                        cache_creation_input_tokens_1h: i64::try_from(
                            usage.cache_creation.ephemeral_1h_input_tokens,
                        )
                        .ok(),
                    })
                }
                _ => None,
            }
        }
        ProtocolKind::OpenAiResponse => {
            use gproxy_protocol::openai::create_response::stream::ResponseStreamEvent;

            let event: ResponseStreamEvent = serde_json::from_slice(chunk).ok()?;
            let response = match event {
                ResponseStreamEvent::Created { response, .. }
                | ResponseStreamEvent::Queued { response, .. }
                | ResponseStreamEvent::InProgress { response, .. }
                | ResponseStreamEvent::Completed { response, .. }
                | ResponseStreamEvent::Incomplete { response, .. }
                | ResponseStreamEvent::Failed { response, .. } => response,
                _ => return None,
            };
            let usage = response.usage?;
            Some(Usage {
                input_tokens: i64::try_from(usage.input_tokens).ok(),
                output_tokens: i64::try_from(usage.output_tokens).ok(),
                cache_read_input_tokens: i64::try_from(usage.input_tokens_details.cached_tokens)
                    .ok(),
                cache_creation_input_tokens: None,
                cache_creation_input_tokens_5min: None,
                cache_creation_input_tokens_1h: None,
            })
        }
        ProtocolKind::OpenAi => {
            use gproxy_protocol::openai::create_image::stream::ImageGenerationStreamEvent;

            let event: ImageGenerationStreamEvent = serde_json::from_slice(chunk).ok()?;
            match event {
                ImageGenerationStreamEvent::Completed { usage, .. } => Some(Usage {
                    input_tokens: i64::try_from(usage.input_tokens).ok(),
                    output_tokens: i64::try_from(usage.output_tokens).ok(),
                    cache_read_input_tokens: None,
                    cache_creation_input_tokens: None,
                    cache_creation_input_tokens_5min: None,
                    cache_creation_input_tokens_1h: None,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}
