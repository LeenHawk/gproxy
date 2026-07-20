//! Convert the `/f/conversation` SSE v1 stream into OpenAI
//! `chat.completion.chunk` events.
//!
//! Channels surfaced downstream (calibrated against live chatgpt.com captures,
//! June 2026 — see `dev-docs/windows-edge-chatgpt-remote-debugging.md`):
//!
//! * the assistant's **final answer** channel — the first "add" whose message
//!   has `author.role == "assistant"`, `content.content_type == "text"`, and
//!   `status != finished_successfully`. Its text arrives as `delta.content`.
//! * the assistant's **reasoning** (chain-of-thought) channel — an "add" whose
//!   message is `author.role == "assistant"` with `content.content_type` in
//!   `{thoughts, reasoning_recap}`. Surfaced as `delta.reasoning_content` (the
//!   DeepSeek / v2-claude-thinking convention), which the transform layer maps
//!   to Claude `thinking` / Gemini thought. Two real shapes:
//!     - `thoughts`: `content.thoughts` is an array of `{summary, content}`
//!       (populated for deep-research; standard thinking models WITHHOLD it and
//!       leave the array empty).
//!     - `reasoning_recap`: `content.content` is a short string (e.g.
//!       "已思考 5s" / "Thought for 5s") present already in the add event.
//!
//! **Path elision (v1 delta encoding).** After an `append` to a path, the
//! server elides repeated metadata: a follow-up event of just `{"v": "more"}`
//! (no `p`, no `o`) means "append `more` to the same path as the previous op".
//! These bare appends carry the bulk of a streamed answer, so dropping them
//! truncates output to the first fragment. They are matched here as an
//! empty-path append on the current channel.
//!
//! Ported from v1 `channels/chatgpt/sse_to_openai.rs`; the reasoning channel and
//! path-elision handling are the v2 enhancements (v1 dropped both). Id/clock are
//! sourced from `crate::util::rand` / `crate::util::time` for wasm-portability.

use serde_json::{Value, json};

use super::sse::{Delta, Event, InitialAddValue, PatchKind};

mod encoding;

pub use encoding::{OpenAiChunk, OpenAiChunkChoice};
use encoding::{reasoning_text, thoughts_array_text};

/// State machine that consumes SSE v1 events and emits OpenAI chat chunks.
#[derive(Debug, Default)]
pub struct SseToOpenAi {
    /// Channel id that carries the assistant's final answer text.
    final_channel: Option<u64>,
    /// Channel id that carries the assistant's reasoning / chain-of-thought.
    reasoning_channel: Option<u64>,
    /// Channel id of the most-recently-added delta event (for follow-up
    /// patches that omit the `c` field).
    current_channel: Option<u64>,
    /// Assistant message id for the final channel.
    message_id: Option<String>,
    /// Target upstream model slug (seeded from initial metadata).
    model: String,
    /// Did we already emit the role delta?
    emitted_role: bool,
    /// Are we done streaming?
    finished: bool,
    /// Accumulated final-answer text (for potential non-streaming aggregation).
    accumulated_text: String,
    /// Accumulated reasoning text (for replace-delta diffing).
    accumulated_reasoning: String,
    /// Inside a web-search citation marker run (`U+E200 … U+E201`) — tracked
    /// across deltas because a marker can split across chunks.
    in_citation: bool,
}

impl SseToOpenAi {
    pub fn new() -> Self {
        Self {
            model: "gpt-5".to_string(),
            ..Default::default()
        }
    }

    #[cfg(test)]
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn emitted_role(&self) -> bool {
        self.emitted_role
    }

    /// Feed one SSE event and return at most one OpenAI chunk.
    ///
    /// Collapses all of the event's patch effects (role/content/finish)
    /// into a single `chat.completion.chunk`. Returns `None` for events
    /// that don't carry anything the client needs — version banner,
    /// typed side events, patches against non-final channels, etc.
    pub fn on_event(&mut self, event: Event) -> Option<OpenAiChunk> {
        match event {
            Event::Delta(delta) => self.on_delta(delta),
            Event::Done => {
                if self.emitted_role && !self.finished {
                    self.finished = true;
                    Some(self.build_stop_chunk("stop"))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn on_delta(&mut self, delta: Delta) -> Option<OpenAiChunk> {
        // "add" event declares a new channel and provides the initial
        // message state.
        let is_add = delta
            .patches
            .first()
            .is_some_and(|p| p.op == PatchKind::Add && p.path.is_empty());
        if let Some(c) = delta.channel {
            self.current_channel = Some(c);
        }

        if is_add {
            let first = &delta.patches[0];
            return self.handle_add(delta.channel, &first.value);
        }

        // Determine which assistant channel these patches target.
        let channel = delta.channel.or(self.current_channel);
        let is_final = matches!((channel, self.final_channel), (Some(c), Some(t)) if c == t);
        let is_reasoning =
            matches!((channel, self.reasoning_channel), (Some(c), Some(t)) if c == t);
        if !is_final && !is_reasoning {
            return None;
        }

        if is_reasoning {
            self.on_reasoning_delta(delta)
        } else {
            self.on_final_delta(delta)
        }
    }

    /// Patches on the final-answer channel → `delta.content` (+ finish).
    fn on_final_delta(&mut self, delta: Delta) -> Option<OpenAiChunk> {
        let mut delta_map = serde_json::Map::new();
        let mut finish_reason: Option<String> = None;
        let mut appended_content = String::new();

        for patch in delta.patches {
            // Path-elided bare append: `{"v": "more"}` (empty path, no explicit
            // op) continues the previous append on this channel — its content
            // path. This carries the bulk of a streamed answer.
            let is_bare_append = patch.path.is_empty()
                && matches!(patch.op, PatchKind::Append | PatchKind::Unknown)
                && patch.value.is_string();
            if is_bare_append {
                if let Some(text) = patch.value.as_str() {
                    appended_content.push_str(text);
                }
                continue;
            }
            match (patch.op, patch.path.as_str()) {
                (PatchKind::Append, "/message/content/parts/0") => {
                    if let Some(text) = patch.value.as_str() {
                        appended_content.push_str(text);
                    }
                }
                (PatchKind::Replace, "/message/content/parts/0") => {
                    if let Some(text) = patch.value.as_str() {
                        // Emit only the new portion relative to what we have.
                        let new_part = if text.starts_with(&self.accumulated_text) {
                            text[self.accumulated_text.len()..].to_string()
                        } else {
                            text.to_string()
                        };
                        appended_content.push_str(&new_part);
                    }
                }
                (PatchKind::Replace, "/message/status")
                    if patch.value.as_str() == Some("finished_successfully") =>
                {
                    finish_reason = Some("stop".to_string());
                    self.finished = true;
                }
                _ => {}
            }
        }

        if !appended_content.is_empty() {
            self.accumulated_text.push_str(&appended_content);
            // Strip web-search citation markers (`U+E200 cite U+E202 … U+E201`)
            // — raw they reach the client as garbage PUA codepoints. The model's
            // own "Sources" list (plain text) survives. accumulated_text keeps
            // the raw text so replace-diffing stays consistent.
            let cleaned = self.strip_citations(&appended_content);
            if !cleaned.is_empty() {
                if !self.emitted_role {
                    delta_map.insert("role".to_string(), json!("assistant"));
                    self.emitted_role = true;
                }
                delta_map.insert("content".to_string(), json!(cleaned));
            }
        }
        if !delta_map.contains_key("content") && finish_reason.is_some() && !self.emitted_role {
            // Edge case: stream ended before any (visible) content arrived.
            delta_map.insert("role".to_string(), json!("assistant"));
            self.emitted_role = true;
        }

        if delta_map.is_empty() && finish_reason.is_none() {
            return None;
        }

        Some(self.build_chunk(delta_map, finish_reason.as_deref()))
    }

    /// Patches on the reasoning channel → `delta.reasoning_content`.
    ///
    /// Calibrated against live captures: standard thinking models leave the
    /// `thoughts` array empty (CoT withheld) and only emit a `reasoning_recap`
    /// (handled at add time in [`Self::handle_add`]); deep-research streams real
    /// `{summary, content}` thought objects. We accept every reasoning-bearing
    /// shape: bare path-elided appends, `parts/0`, and the `/thoughts/…` paths.
    fn on_reasoning_delta(&mut self, delta: Delta) -> Option<OpenAiChunk> {
        let mut appended = String::new();

        for patch in delta.patches {
            // Bare path-elided append (`{"v": "more"}`) continues the previous
            // reasoning append.
            if patch.path.is_empty()
                && matches!(patch.op, PatchKind::Append | PatchKind::Unknown)
                && let Some(text) = reasoning_text(&patch.value)
            {
                appended.push_str(&text);
                continue;
            }
            let is_reasoning_path = patch.path == "/message/content/parts/0"
                || patch.path.starts_with("/message/content/thoughts")
                || patch.path == "/message/content/content";
            if !is_reasoning_path {
                continue;
            }
            match patch.op {
                PatchKind::Append | PatchKind::Add => {
                    if let Some(text) = reasoning_text(&patch.value) {
                        appended.push_str(&text);
                    }
                }
                PatchKind::Replace => {
                    if let Some(text) = reasoning_text(&patch.value) {
                        let new_part = if text.starts_with(&self.accumulated_reasoning) {
                            text[self.accumulated_reasoning.len()..].to_string()
                        } else {
                            text
                        };
                        appended.push_str(&new_part);
                    }
                }
                _ => {}
            }
        }

        if appended.is_empty() {
            return None;
        }

        self.accumulated_reasoning.push_str(&appended);
        Some(self.build_reasoning_chunk(&appended))
    }

    /// Build a chunk carrying `reasoning_content` (emits the role delta once).
    fn build_reasoning_chunk(&mut self, text: &str) -> OpenAiChunk {
        let mut delta_map = serde_json::Map::new();
        if !self.emitted_role {
            delta_map.insert("role".to_string(), json!("assistant"));
            self.emitted_role = true;
        }
        delta_map.insert("reasoning_content".to_string(), json!(text));
        self.build_chunk(delta_map, None)
    }

    /// Remove ChatGPT web-search citation markers from streamed text. Citations
    /// are wrapped in a Private-Use run: `U+E200` (start) `cite` `U+E202` (sep)
    /// `turn0search0` … `U+E201` (end). Raw, these reach the client as garbage.
    /// `in_citation` carries across calls so a marker split across delta chunks
    /// is still fully stripped. Stray controls in `U+E200..=U+E20F` are dropped.
    fn strip_citations(&mut self, text: &str) -> String {
        const CITE_START: u32 = 0xE200;
        const CITE_END: u32 = 0xE201;
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            let cp = ch as u32;
            if self.in_citation {
                if cp == CITE_END {
                    self.in_citation = false;
                }
                continue;
            }
            if cp == CITE_START {
                self.in_citation = true;
                continue;
            }
            if (0xE200..=0xE20F).contains(&cp) {
                continue; // stray citation control
            }
            out.push(ch);
        }
        out
    }

    /// Handle an "add" event declaring a new channel. Returns a reasoning chunk
    /// when the added channel already carries reasoning text (a `reasoning_recap`
    /// label, or a pre-populated `thoughts` array), otherwise `None`.
    fn handle_add(&mut self, channel: Option<u64>, value: &Value) -> Option<OpenAiChunk> {
        let wrap: InitialAddValue = serde_json::from_value(value.clone()).unwrap_or_default();
        let msg = wrap.message?;

        // Inspect message to see which assistant channel this is.
        let role = msg
            .get("author")
            .and_then(|a| a.get("role"))
            .and_then(|v| v.as_str());
        let content = msg.get("content");
        let content_type = content
            .and_then(|c| c.get("content_type"))
            .and_then(|v| v.as_str());
        let status = msg.get("status").and_then(|v| v.as_str());
        let hidden = msg
            .get("metadata")
            .and_then(|m| m.get("is_visually_hidden_from_conversation"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if role != Some("assistant") || status == Some("finished_successfully") {
            // A `reasoning_recap` arrives already-`finished_successfully` with its
            // label in `content.content`; surface it even though it's "finished".
            if role == Some("assistant") && content_type == Some("reasoning_recap") {
                self.reasoning_channel = self.reasoning_channel.or(channel);
                if let Some(text) = content
                    .and_then(|c| c.get("content"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    self.accumulated_reasoning.push_str(text);
                    return Some(self.build_reasoning_chunk(text));
                }
            }
            return None;
        }

        // Reasoning channel: chain-of-thought content types.
        if matches!(content_type, Some("thoughts") | Some("reasoning_recap")) {
            if self.reasoning_channel.is_none() {
                self.reasoning_channel = channel;
            }
            // A pre-populated `thoughts` array (deep-research) carries real CoT.
            if let Some(text) = thoughts_array_text(content) {
                self.accumulated_reasoning.push_str(&text);
                return Some(self.build_reasoning_chunk(&text));
            }
            return None;
        }

        // Final-answer channel: visible assistant text.
        let is_assistant_text = content_type == Some("text") && !hidden;
        if is_assistant_text && self.final_channel.is_none() {
            self.final_channel = channel;
            self.message_id = msg.get("id").and_then(|v| v.as_str()).map(String::from);
            if let Some(model) = msg
                .get("metadata")
                .and_then(|m| m.get("model_slug"))
                .and_then(|v| v.as_str())
            {
                self.model = model.to_string();
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
                .unwrap_or_else(|| format!("chatcmpl-{}", crate::util::rand::uuid_v4())),
            object: "chat.completion.chunk",
            created: crate::util::time::unix_now_ms() / 1000,
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
#[cfg(test)]
pub fn collect_all(model: &str, body: &[u8]) -> Vec<OpenAiChunk> {
    use super::sse::SseDecoder;
    let mut decoder = SseDecoder::new();
    let mut converter = SseToOpenAi::with_model(model);
    let mut out = Vec::new();
    decoder.feed(body);
    while let Some(event) = decoder.next_event() {
        if let Some(chunk) = converter.on_event(event) {
            out.push(chunk);
        }
    }
    // Trailer: emit a synthesized stop if the upstream never sent one.
    if !converter.finished() && converter.emitted_role {
        out.push(converter.build_stop_chunk("stop"));
    }
    out
}

#[cfg(test)]
mod tests;
