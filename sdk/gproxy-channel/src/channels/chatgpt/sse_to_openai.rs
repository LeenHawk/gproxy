//! Convert the `/f/conversation` SSE v1 stream into OpenAI
//! `chat.completion.chunk` events.
//!
//! Only events that belong to the **assistant's final answer channel** are
//! emitted downstream. That channel is identified on-the-fly as the first
//! "add" event whose message has `author.role == "assistant"`,
//! `content.content_type == "text"`, and `status == "in_progress"`.

use serde::Serialize;
use serde_json::{Value, json};

use super::sse_v1::{Delta, Event, InitialAddValue, PatchKind, PatchOp, SseDecoder};

/// State machine that consumes SSE v1 events and emits OpenAI chat chunks.
#[derive(Debug, Default)]
pub struct SseToOpenAi {
    /// Channel id that carries the assistant's final answer text.
    final_channel: Option<u64>,
    /// Channel id of the most-recently-added delta event (for follow-up
    /// patches that omit the `c` field).
    current_channel: Option<u64>,
    /// Conversation id (taken from the first add).
    conversation_id: Option<String>,
    /// Assistant message id for the final channel.
    message_id: Option<String>,
    /// Target upstream model slug (seeded from initial metadata).
    model: String,
    /// Did we already emit the role delta?
    emitted_role: bool,
    /// Are we done streaming?
    finished: bool,
    /// Accumulated text (for potential non-streaming aggregation).
    accumulated_text: String,
}

/// One OpenAI `chat.completion.chunk` value.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChunk {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChunkChoice>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChunkChoice {
    pub index: u32,
    pub delta: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

impl SseToOpenAi {
    pub fn new() -> Self {
        Self {
            model: "gpt-5".to_string(),
            ..Default::default()
        }
    }

    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Full accumulated text (useful for non-streaming aggregation).
    pub fn text(&self) -> &str {
        &self.accumulated_text
    }

    pub fn conversation_id(&self) -> Option<&str> {
        self.conversation_id.as_deref()
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    /// Feed one SSE event and collect any OpenAI chunks it produces.
    pub fn on_event(&mut self, event: Event) -> Vec<OpenAiChunk> {
        match event {
            Event::Delta(delta) => self.on_delta(delta),
            Event::Done => {
                // Synthesize final stop chunk if we never saw one.
                if self.emitted_role && !self.finished {
                    self.finished = true;
                    return vec![self.build_stop_chunk("stop")];
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_delta(&mut self, delta: Delta) -> Vec<OpenAiChunk> {
        // Update the current channel when an "add" declares one.
        let is_add = delta
            .patches
            .first()
            .is_some_and(|p| p.op == PatchKind::Add && p.path.is_empty());
        if let Some(c) = delta.channel {
            self.current_channel = Some(c);
        }

        if is_add {
            let first = &delta.patches[0];
            self.handle_add(delta.channel, &first.value);
            return Vec::new();
        }

        // Patch events: only relevant for the assistant's final channel.
        let relevant = match (delta.channel, self.current_channel, self.final_channel) {
            (Some(c), _, Some(target)) => c == target,
            (None, Some(cur), Some(target)) => cur == target,
            _ => false,
        };
        if !relevant {
            return Vec::new();
        }

        let mut out = Vec::new();
        for patch in delta.patches {
            if let Some(chunk) = self.handle_patch(&patch) {
                out.push(chunk);
            }
        }
        out
    }

    fn handle_add(&mut self, channel: Option<u64>, value: &Value) {
        let wrap: InitialAddValue =
            serde_json::from_value(value.clone()).unwrap_or_default();
        if let Some(cid) = wrap.conversation_id {
            self.conversation_id = Some(cid);
        }
        let msg = match wrap.message {
            Some(m) => m,
            None => return,
        };

        // Inspect message to see if this is the assistant's final answer.
        let role = msg
            .get("author")
            .and_then(|a| a.get("role"))
            .and_then(|v| v.as_str());
        let content_type = msg
            .get("content")
            .and_then(|c| c.get("content_type"))
            .and_then(|v| v.as_str());
        let status = msg.get("status").and_then(|v| v.as_str());

        let is_assistant_text = role == Some("assistant")
            && content_type == Some("text")
            && status != Some("finished_successfully")
            && !msg
                .get("metadata")
                .and_then(|m| m.get("is_visually_hidden_from_conversation"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        if is_assistant_text && self.final_channel.is_none() {
            self.final_channel = channel;
            self.message_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(model) = msg
                .get("metadata")
                .and_then(|m| m.get("model_slug"))
                .and_then(|v| v.as_str())
            {
                self.model = model.to_string();
            }
        }
    }

    fn handle_patch(&mut self, patch: &PatchOp) -> Option<OpenAiChunk> {
        // Text streaming: append to /message/content/parts/0.
        if patch.op == PatchKind::Append && patch.path == "/message/content/parts/0" {
            let text = patch.value.as_str()?.to_string();
            if text.is_empty() {
                return None;
            }
            self.accumulated_text.push_str(&text);
            let mut delta = serde_json::Map::new();
            if !self.emitted_role {
                delta.insert("role".to_string(), json!("assistant"));
                self.emitted_role = true;
            }
            delta.insert("content".to_string(), json!(text));
            return Some(self.build_chunk(delta, None));
        }

        // Full-content replace: `/message/content/parts/0` replaced with
        // a new string (some models send this instead of append early on).
        if patch.op == PatchKind::Replace && patch.path == "/message/content/parts/0" {
            let text = patch.value.as_str()?.to_string();
            if text.is_empty() {
                return None;
            }
            // Emit delta of just the new portion relative to what we have.
            let new_delta = if text.starts_with(&self.accumulated_text) {
                text[self.accumulated_text.len()..].to_string()
            } else {
                text.clone()
            };
            self.accumulated_text = text;
            if new_delta.is_empty() {
                return None;
            }
            let mut delta = serde_json::Map::new();
            if !self.emitted_role {
                delta.insert("role".to_string(), json!("assistant"));
                self.emitted_role = true;
            }
            delta.insert("content".to_string(), json!(new_delta));
            return Some(self.build_chunk(delta, None));
        }

        // End of stream.
        if patch.op == PatchKind::Replace && patch.path == "/message/status" {
            if patch.value.as_str() == Some("finished_successfully") {
                self.finished = true;
                return Some(self.build_stop_chunk("stop"));
            }
        }

        None
    }

    fn build_chunk(
        &self,
        delta: serde_json::Map<String, Value>,
        finish_reason: Option<&str>,
    ) -> OpenAiChunk {
        OpenAiChunk {
            id: self
                .message_id
                .clone()
                .unwrap_or_else(|| format!("chatcmpl-{}", uuid::Uuid::new_v4())),
            object: "chat.completion.chunk",
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            model: self.model.clone(),
            choices: vec![OpenAiChunkChoice {
                index: 0,
                delta,
                finish_reason: finish_reason.map(String::from),
            }],
        }
    }

    fn build_stop_chunk(&self, reason: &str) -> OpenAiChunk {
        self.build_chunk(serde_json::Map::new(), Some(reason))
    }
}

/// Convenience: buffer an entire SSE response and collect all emitted
/// OpenAI chunks in order. Used by tests and non-streaming callers.
pub fn collect_all(model: &str, body: &[u8]) -> Vec<OpenAiChunk> {
    let mut decoder = SseDecoder::new();
    let mut converter = SseToOpenAi::with_model(model);
    let mut out = Vec::new();
    decoder.feed(body);
    while let Some(event) = decoder.next_event() {
        out.extend(converter.on_event(event));
    }
    // Trailer: emit a synthesized stop if the upstream never sent one.
    if !converter.finished() && converter.emitted_role {
        out.push(converter.build_stop_chunk("stop"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_text_sse() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/samples/05_sse_response_text.txt"
        ))
        .unwrap_or_default()
    }

    #[test]
    fn reconstructs_text_from_real_har() {
        let bytes = load_text_sse();
        if bytes.is_empty() {
            eprintln!("skip: samples not present");
            return;
        }
        let chunks = collect_all("gpt-5", &bytes);
        assert!(!chunks.is_empty(), "should produce at least one chunk");

        let text: String = chunks
            .iter()
            .flat_map(|c| c.choices.iter())
            .filter_map(|ch| ch.delta.get("content").and_then(|v| v.as_str()))
            .collect();

        assert!(text.contains("冒泡") || text.contains("bubble"),
            "expected bubble sort text, got: {}", text.chars().take(100).collect::<String>());

        // Last chunk should carry a finish_reason.
        let finishes: Vec<_> = chunks
            .iter()
            .filter_map(|c| c.choices.first().and_then(|ch| ch.finish_reason.as_deref()))
            .collect();
        assert!(
            finishes.last() == Some(&"stop"),
            "expected final chunk to have finish_reason=stop"
        );
    }

    #[test]
    fn ignores_system_and_user_channels() {
        let body = br#"event: delta_encoding
data: "v1"

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"sys","author":{"role":"system"},"content":{"content_type":"text","parts":[""]},"status":"finished_successfully","metadata":{"is_visually_hidden_from_conversation":true}},"conversation_id":"c1"},"c":0}

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"user","author":{"role":"user"},"content":{"content_type":"text","parts":["hi"]},"status":"finished_successfully"},"conversation_id":"c1"},"c":1}

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"asst","author":{"role":"assistant"},"content":{"content_type":"text","parts":[""]},"status":"in_progress","metadata":{"model_slug":"gpt-5"}},"conversation_id":"c1"},"c":2}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":"hello"}]}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":" world"},{"p":"/message/status","o":"replace","v":"finished_successfully"}]}

"#;
        let chunks = collect_all("gpt-5", body);
        let text: String = chunks
            .iter()
            .flat_map(|c| c.choices.iter())
            .filter_map(|ch| ch.delta.get("content").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(text, "hello world");
        assert!(chunks.last().unwrap().choices[0].finish_reason.as_deref() == Some("stop"));
    }
}
