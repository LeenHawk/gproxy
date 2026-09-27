//! OpenAI Chat Completions and Responses usage, buffered, streamed over SSE,
//! and carried over a websocket by Responses and realtime sessions.
//!
//! Two usage shapes exist and one reader takes both, recognizing the shape
//! from its field names. Chat counts `prompt_tokens` / `completion_tokens`
//! with details under `prompt_tokens_details` / `completion_tokens_details`;
//! Responses counts `input_tokens` / `output_tokens` with details under
//! `input_tokens_details` / `output_tokens_details`, which realtime spells
//! `input_token_details` / `output_token_details`. Chat is recognized first
//! because a Responses object never carries `prompt_tokens`.
//!
//! Both prompt totals include the cached read (`cached_tokens`) and, where a
//! compatible vendor reports one, the cache write (`cache_write_tokens`).
//! The normalized input is the ordinary input alone, so both come back out
//! of it and ride in the fields that name them; a cache write has no TTL on
//! this wire and is carried in the 30-minute bucket. Per-modality details
//! (`audio_tokens`, `text_tokens`, `image_tokens`, and the cached
//! `cached_tokens_details` breakdown) are recorded as metrics that are
//! subsets of the totals, and the reading says so with the
//! `token_modalities_in_totals` dimension, which is how pricing knows to take
//! them out of the totals instead of charging them twice.
//!
//! The serving tier is `service_tier` at the root of a reply or on the
//! `response` of an event, a string or an object with a `type`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use super::common::{self, count, present};
use super::media;
use super::types::{NormalizedUsage, ResponseUsage, UsageCompleteness};

const MODALITIES: [&str; 3] = ["audio", "text", "image"];

/// A `usage` object in either shape, or `None` when it reports neither.
pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let chat = present(usage, "prompt_tokens")
        && (present(usage, "completion_tokens") || present(usage, "total_tokens"));
    let (prompt, completion, input_names, output_names): (_, _, &[&str], &[&str]) = if chat {
        (
            "prompt_tokens",
            "completion_tokens",
            &["prompt_tokens_details"],
            &["completion_tokens_details"],
        )
    } else if present(usage, "input_tokens") && present(usage, "output_tokens") {
        (
            "input_tokens",
            "output_tokens",
            &["input_tokens_details", "input_token_details"],
            &["output_tokens_details", "output_token_details"],
        )
    } else {
        return None;
    };
    let empty = Value::Null;
    let details = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| usage.get(*name).filter(|value| value.is_object()))
            .unwrap_or(&empty)
    };
    let input = details(input_names);
    let output = details(output_names);
    let cached = count(input, "cached_tokens");
    let cache_write = count(input, "cache_write_tokens");
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = Some(
        count(usage, prompt)
            .unwrap_or_default()
            .saturating_sub(cached.unwrap_or_default())
            .saturating_sub(cache_write.unwrap_or_default()),
    );
    normalized.tokens.output_tokens = Some(count(usage, completion).unwrap_or_default());
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.cache_creation_30m_tokens = cache_write;
    normalized.tokens.reasoning_tokens = count(output, "reasoning_tokens");
    for modality in MODALITIES {
        let tokens = format!("{modality}_tokens");
        for (side, details) in [("input", input), ("output", output)] {
            common::metric(
                &mut normalized,
                &format!("{modality}_{side}_tokens"),
                count(details, &tokens).unwrap_or_default(),
            );
        }
        if let Some(cached) = input.get("cached_tokens_details") {
            common::metric(
                &mut normalized,
                &format!("cached_{modality}_input_tokens"),
                count(cached, &tokens).unwrap_or_default(),
            );
        }
    }
    common::flag_modalities(&mut normalized);
    let searches = usage
        .get("server_tool_use_details")
        .or_else(|| usage.get("server_tool_use"))
        .and_then(|tools| count(tools, "web_search_requests"))
        .unwrap_or_default();
    common::metric(&mut normalized, "web_searches", searches);
    Some(normalized)
}

/// What a reply or event carries around its usage.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default, deserialize_with = "common::string")]
    id: Option<String>,
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
    #[serde(default, deserialize_with = "common::any")]
    service_tier: Option<Value>,
}

/// A buffered reply: the usage is at the root, or under `response` in an
/// event-shaped body.
#[derive(Deserialize)]
struct Reply {
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
    #[serde(default, deserialize_with = "common::any")]
    service_tier: Option<Value>,
    #[serde(default)]
    response: Option<Envelope>,
}

/// Usage from a buffered Chat Completions or Responses reply.
pub(super) fn whole(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    let (usage, tier) = match reply.usage {
        Some(usage) => (
            usage,
            reply
                .service_tier
                .or_else(|| reply.response.and_then(|response| response.service_tier)),
        ),
        None => {
            let response = reply.response?;
            (response.usage?, response.service_tier)
        }
    };
    let mut normalized = from_usage(&usage)?;
    common::service_tier(&mut normalized, tier.as_ref().and_then(common::label));
    Some(normalized)
}

// ------------------------------------------------------------------- chat

/// Watches a Chat Completions stream. Any chunk may carry a non-null `usage`;
/// with `stream_options.include_usage` the last one does. The counts are
/// cumulative, so the last chunk that reports usage is the reading.
#[derive(Default)]
pub(super) struct ChatStream {
    usage: Option<NormalizedUsage>,
}

impl ChatStream {
    pub(super) fn event(&mut self, data: &str) -> bool {
        // Every other chunk is content; `"usage":null` fails the parse below
        // soon enough, but most chunks do not name usage at all.
        if !data.contains("\"usage\"") {
            return false;
        }
        let Ok(reply) = serde_json::from_str::<Reply>(data) else {
            return false;
        };
        let Some(usage) = reply.usage.as_ref().and_then(from_usage) else {
            return false;
        };
        let mut usage = usage;
        common::service_tier(
            &mut usage,
            reply.service_tier.as_ref().and_then(common::label),
        );
        self.usage = Some(usage);
        true
    }

    pub(super) fn snapshot(&self) -> Option<NormalizedUsage> {
        self.usage.clone()
    }
}

// -------------------------------------------------------------- responses

/// What a Responses-family stream is expected to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// One response over SSE or a Responses websocket. Its terminal event
    /// carries the usage.
    Single,
    /// A realtime session: many responses, each identified by its upstream
    /// id, summed into one reading.
    Realtime,
    /// A streamed image generation or edit: the `*.completed` event carries
    /// the usage at its root.
    Image,
}

/// The events that end a response; each carries `response.usage`.
const TERMINAL: [&str; 4] = [
    "response.completed",
    "response.incomplete",
    "response.failed",
    "response.done",
];
const IMAGE_DONE: [&str; 2] = ["image_generation.completed", "image_edit.completed"];
const CREATED: &str = "response.created";

/// One stream event, reduced to what metering reads.
#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type", default, deserialize_with = "common::string")]
    kind: Option<String>,
    #[serde(default)]
    response: Option<Envelope>,
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
    #[serde(default, deserialize_with = "common::string")]
    generation_id: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    size: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    quality: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    output_format: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    background: Option<String>,
}

/// Watches a Responses stream, a Responses websocket, a realtime session or
/// a streamed image.
pub(super) struct ResponsesStream {
    mode: Mode,
    usage: Option<NormalizedUsage>,
    /// Realtime responses by upstream id. A repeated terminal event replaces
    /// its response rather than charging it again.
    responses: BTreeMap<String, NormalizedUsage>,
    /// Realtime responses started or ended without usage, which leave the
    /// session's reading partial.
    active: BTreeSet<String>,
}

impl ResponsesStream {
    pub(super) fn new(mode: Mode) -> Self {
        Self {
            mode,
            usage: None,
            responses: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }

    fn watches(&self, kind: &str) -> bool {
        TERMINAL.contains(&kind)
            || (self.mode == Mode::Realtime && kind == CREATED)
            || (self.mode == Mode::Image && IMAGE_DONE.contains(&kind))
    }

    pub(super) fn event(&mut self, name: Option<&str>, data: &str) -> bool {
        // SSE names the event; a websocket frame only says it in `type`, so
        // the data must mention one of the watched events to be parsed.
        if name.is_some_and(|name| !self.watches(name)) {
            return false;
        }
        let mentioned = TERMINAL
            .iter()
            .chain(&IMAGE_DONE)
            .chain(&[CREATED])
            .any(|kind| data.contains(kind) && self.watches(kind));
        if !mentioned {
            return false;
        }
        let Ok(mut event) = serde_json::from_str::<Event>(data) else {
            return false;
        };
        let Some(kind) = event.kind.take().filter(|kind| self.watches(kind)) else {
            return false;
        };
        let kind = kind.as_str();
        if kind == CREATED {
            if let Some(id) = event.response.and_then(|response| response.id) {
                self.active.insert(id);
            }
            return false;
        }
        let image = IMAGE_DONE.contains(&kind);
        let (id, usage, tier) = if image {
            (event.generation_id.clone(), event.usage.clone(), None)
        } else {
            let Some(response) = event.response.take() else {
                return false;
            };
            (response.id, response.usage, response.service_tier)
        };
        let reading = usage.as_ref().and_then(|usage| {
            if self.mode == Mode::Image {
                media::image_usage(usage)
            } else {
                from_usage(usage)
            }
        });
        let Some(mut reading) = reading else {
            if self.mode == Mode::Realtime
                && let Some(id) = id
            {
                self.active.insert(id);
            }
            return false;
        };
        common::service_tier(&mut reading, tier.as_ref().and_then(common::label));
        if image {
            common::metric(&mut reading, "image_outputs", 1);
            media::qualify(
                &mut reading,
                [
                    ("size", event.size),
                    ("quality", event.quality),
                    ("output_format", event.output_format),
                    ("background", event.background),
                ],
            );
        }
        if self.mode != Mode::Realtime {
            self.usage = Some(reading);
            return true;
        }
        // Native response ids make repeated terminal events replacements,
        // not additional charges. Never invent an id for billable usage.
        let Some(id) = id else {
            return false;
        };
        self.active.remove(&id);
        self.responses.insert(id, reading);
        let mut total = NormalizedUsage::aggregate(self.responses.values());
        total.responses = self
            .responses
            .iter()
            .map(|(id, usage)| ResponseUsage {
                id: id.clone(),
                usage: Box::new(usage.clone()),
            })
            .collect();
        self.usage = Some(total);
        true
    }

    pub(super) fn snapshot(&self) -> Option<NormalizedUsage> {
        let mut usage = self.usage.clone()?;
        if !self.active.is_empty() {
            usage.completeness = UsageCompleteness::Partial;
        }
        Some(usage)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{chunked, feed_messages, feed_sse};
    use super::super::{UsageStreamEnd, whole as read_whole};
    use super::*;
    use crate::{Dialect, Operation};
    use serde_json::json;

    fn buffered(dialect: Dialect, body: &Value) -> Option<NormalizedUsage> {
        read_whole(
            Operation::GenerateContent,
            dialect,
            body.to_string().as_bytes(),
        )
    }

    #[test]
    fn chat_reads_cache_reads_writes_reasoning_and_modalities() {
        let usage = buffered(
            Dialect::OpenAiChat,
            &json!({"service_tier": "flex", "choices": [], "usage": {
                "prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120,
                "prompt_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 3, "audio_tokens": 5},
                "completion_tokens_details": {"reasoning_tokens": 7, "audio_tokens": 0}}}),
        )
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(57));
        assert_eq!(usage.tokens.output_tokens, Some(20));
        assert_eq!(usage.tokens.cached_input_tokens, Some(40));
        assert_eq!(usage.tokens.cache_creation_30m_tokens, Some(3));
        assert_eq!(usage.tokens.reasoning_tokens, Some(7));
        assert_eq!(usage.metrics["audio_input_tokens"], 5.into());
        assert!(!usage.metrics.contains_key("audio_output_tokens"), "zero");
        assert_eq!(usage.dimensions[common::MODALITIES_IN_TOTALS], "true");
        assert_eq!(usage.dimensions["service_tier"], "flex");
        assert_eq!(usage.actual_service_tier.as_deref(), Some("flex"));
        assert_eq!(usage.completeness, UsageCompleteness::Complete);

        let bare = buffered(
            Dialect::OpenAiChat,
            &json!({"usage": {"prompt_tokens": 9, "completion_tokens": 4}}),
        )
        .unwrap();
        assert_eq!(
            bare.tokens.input_tokens,
            Some(9),
            "total_tokens is optional"
        );
        assert!(bare.dimensions.is_empty());
        assert_eq!(bare.tokens.cache_creation_30m_tokens, None);
    }

    #[test]
    fn responses_reads_both_detail_spellings_and_web_searches() {
        for details in ["input_tokens_details", "input_token_details"] {
            let usage = buffered(
                Dialect::OpenAi,
                &json!({"service_tier": {"type": "priority"}, "usage": {
                    "input_tokens": 100, "output_tokens": 20,
                    details: {"cached_tokens": 40, "cache_write_tokens": 3},
                    "output_tokens_details": {"reasoning_tokens": 2},
                    "server_tool_use": {"web_search_requests": 3}}}),
            )
            .unwrap();
            assert_eq!(usage.tokens.input_tokens, Some(57));
            assert_eq!(usage.tokens.cache_creation_30m_tokens, Some(3));
            assert_eq!(usage.tokens.reasoning_tokens, Some(2));
            assert_eq!(usage.metrics["web_searches"], 3.into());
            assert_eq!(usage.actual_service_tier.as_deref(), Some("priority"));
        }
        let zero = buffered(
            Dialect::OpenAi,
            &json!({"usage": {"input_tokens": 10, "output_tokens": 2,
                "input_tokens_details": {"cache_write_tokens": 0}}}),
        )
        .unwrap();
        assert_eq!(
            zero.tokens.cache_creation_30m_tokens,
            Some(0),
            "an explicit zero stays one"
        );
        assert!(buffered(Dialect::OpenAi, &json!({"id": "resp_1"})).is_none());
        assert!(buffered(Dialect::OpenAi, &json!({"usage": null})).is_none());
    }

    #[test]
    fn a_chat_stream_takes_the_last_usage_chunk_in_any_chunking() {
        let wire = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"the \\\"usage\\\" word\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\n",
            "data: {\"choices\":[],\"service_tier\":\"default\",\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4,\"total_tokens\":13,\"prompt_tokens_details\":{\"cached_tokens\":2}}}\n\n",
            "data: [DONE]\n\n",
        );
        let expected = feed_sse(
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            &chunked(wire, usize::MAX),
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(expected.tokens.input_tokens, Some(7));
        assert_eq!(expected.tokens.cached_input_tokens, Some(2));
        assert_eq!(expected.tokens.output_tokens, Some(4));
        assert_eq!(expected.actual_service_tier.as_deref(), Some("default"));
        for split in [1, 2, 5, 17, 64] {
            let usage = feed_sse(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &chunked(wire, split),
                UsageStreamEnd::Complete,
            );
            assert_eq!(usage.as_ref(), Some(&expected), "split {split}");
        }
        let interrupted = feed_sse(
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            &chunked(wire, 9),
            UsageStreamEnd::Interrupted,
        )
        .unwrap();
        assert_eq!(interrupted.completeness, UsageCompleteness::Partial);
        let content_only = wire
            .split("data: {\"choices\":[{\"delta\":{}}]")
            .next()
            .unwrap();
        assert!(
            feed_sse(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                &chunked(content_only, 3),
                UsageStreamEnd::Interrupted,
            )
            .is_none(),
            "no usage chunk, no reading"
        );
    }

    #[test]
    fn a_responses_stream_reads_its_terminal_event_in_any_chunking() {
        let wire = concat!(
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\",\"service_tier\":\"auto\"}}\n\n",
            "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"response.completed\"}\n\n",
            "event: response.completed\r\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"output\":[{\"type\":\"message\"}],\"service_tier\":\"priority\",\"usage\":{\"input_tokens\":9,\"output_tokens\":4,\"input_tokens_details\":{\"cached_tokens\":1},\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\r\n\r\n",
        );
        let expected = feed_sse(
            Operation::StreamGenerateContent,
            Dialect::OpenAi,
            &chunked(wire, usize::MAX),
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(expected.tokens.input_tokens, Some(8));
        assert_eq!(expected.tokens.reasoning_tokens, Some(2));
        assert_eq!(expected.actual_service_tier.as_deref(), Some("priority"));
        for split in [1, 3, 7, 50] {
            assert_eq!(
                feed_sse(
                    Operation::StreamGenerateContent,
                    Dialect::OpenAi,
                    &chunked(wire, split),
                    UsageStreamEnd::Complete,
                )
                .as_ref(),
                Some(&expected),
                "split {split}"
            );
        }
        for terminal in ["response.incomplete", "response.failed"] {
            let wire = format!(
                "data: {{\"type\":\"{terminal}\",\"response\":{{\"usage\":{{\"input_tokens\":5,\"output_tokens\":1}}}}}}\n\n"
            );
            let usage = feed_sse(
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                &chunked(&wire, 4),
                UsageStreamEnd::Complete,
            )
            .unwrap();
            assert_eq!(usage.tokens.input_tokens, Some(5), "{terminal}");
        }
        let cut = wire.split("event: response.completed").next().unwrap();
        assert!(
            feed_sse(
                Operation::StreamGenerateContent,
                Dialect::OpenAi,
                &chunked(cut, 4),
                UsageStreamEnd::Interrupted,
            )
            .is_none(),
            "Responses reports usage only at the end"
        );
    }

    #[test]
    fn a_responses_websocket_reads_text_frames() {
        let completed = json!({"type": "response.completed", "response": {"id": "r1",
            "usage": {"input_tokens": 12, "output_tokens": 3}}})
        .to_string();
        let usage = feed_messages(
            Operation::StreamGenerateContent,
            Dialect::OpenAiResponsesWebSocket,
            &[
                r#"{"type":"response.output_text.delta","delta":"x"}"#,
                &completed,
            ],
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(12));
        assert!(usage.responses.is_empty(), "one response, not a session");
    }

    #[test]
    fn realtime_sums_unique_responses_and_tracks_open_ones() {
        let done = json!({"type": "response.done", "response": {"id": "r1", "status": "completed", "usage": {
            "input_tokens": 100, "output_tokens": 30,
            "input_token_details": {"cached_tokens": 20, "audio_tokens": 70, "text_tokens": 30,
                "cached_tokens_details": {"audio_tokens": 15, "text_tokens": 5}},
            "output_token_details": {"audio_tokens": 25, "text_tokens": 5}}}})
        .to_string();
        let second = json!({"type": "response.done", "response": {"id": "r2",
            "usage": {"input_tokens": 10, "output_tokens": 4}}})
        .to_string();
        let created = json!({"type": "response.created", "response": {"id": "r3"}}).to_string();
        let usage = feed_messages(
            Operation::ConnectRealtime,
            Dialect::OpenAi,
            &[&done, &done, &second],
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(90));
        assert_eq!(usage.tokens.output_tokens, Some(34));
        assert_eq!(usage.tokens.cached_input_tokens, Some(20));
        assert_eq!(usage.metrics["audio_input_tokens"], 70.into());
        assert_eq!(usage.metrics["cached_audio_input_tokens"], 15.into());
        assert_eq!(usage.metrics["audio_output_tokens"], 25.into());
        assert_eq!(usage.responses.len(), 2);
        assert_eq!(usage.completeness, UsageCompleteness::Complete);
        let open = feed_messages(
            Operation::ConnectRealtime,
            Dialect::OpenAi,
            &[&done, &created],
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(open.completeness, UsageCompleteness::Partial);
        assert_eq!(open.tokens.input_tokens, Some(80));
    }
}
