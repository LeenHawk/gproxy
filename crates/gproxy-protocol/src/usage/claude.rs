//! Claude Messages usage, buffered and streamed.
//!
//! Claude's `input_tokens` already excludes cache reads and cache creation,
//! so it is the normalized input as reported. `cache_read_input_tokens` is the
//! cached input; the `cache_creation` breakdown fills the 5-minute and 1-hour
//! buckets, and without it the flat `cache_creation_input_tokens` is the
//! default 5-minute TTL. `output_tokens_details.thinking_tokens` is the
//! reasoning subset of the output. Server tools report `web_search_requests`
//! and `web_fetch_requests`; `speed`, `service_tier` and `inference_geo` are
//! pricing qualifiers, given as a string or as an object with a `name`.
//!
//! Two things turn one response into several billing attempts. A server-side
//! fallback reports each model it tried under `usage.iterations`; each tried
//! model is an attempt priced at its own rate, billable when it produced
//! output or when it is the last one and the answer was not refused. A plain
//! refusal is one attempt, billable only if it produced output. An ordinary
//! answer carries no attempts: the exchange's own model prices it.
//!
//! A stream splits the reading. `message_start` carries the input side and a
//! provisional output; `message_delta` carries the final output and, after a
//! fallback, restates the input side. The delta wins wherever it reports a
//! count, except that a flat cache-creation total does not overwrite a
//! breakdown `message_start` already gave: the total restates it, it is not a
//! new 5-minute measurement. A stream that ends before any `message_delta`
//! reports the input side as a `Partial` reading rather than nothing.

use serde::Deserialize;
use serde_json::{Map, Value};

use super::common::{self, count};
use super::types::{NormalizedUsage, UsageAttempt, UsageCompleteness};

/// A Messages `usage` object. At least one side must be reported; a side the
/// object leaves out stays unknown.
pub(super) fn from_usage(usage: &Value) -> Option<NormalizedUsage> {
    let input = count(usage, "input_tokens");
    let output = count(usage, "output_tokens");
    if input.is_none() && output.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = input;
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = count(usage, "cache_read_input_tokens");
    match usage
        .get("cache_creation")
        .filter(|value| value.is_object())
    {
        Some(cache) => {
            normalized.tokens.cache_creation_5m_tokens = count(cache, "ephemeral_5m_input_tokens");
            normalized.tokens.cache_creation_1h_tokens = count(cache, "ephemeral_1h_input_tokens");
        }
        None => {
            normalized.tokens.cache_creation_5m_tokens =
                count(usage, "cache_creation_input_tokens");
        }
    }
    normalized.tokens.reasoning_tokens = usage
        .get("output_tokens_details")
        .and_then(|details| count(details, "thinking_tokens"));
    if let Some(tools) = usage.get("server_tool_use") {
        common::metric(
            &mut normalized,
            "web_searches",
            count(tools, "web_search_requests").unwrap_or_default(),
        );
        common::metric(
            &mut normalized,
            "web_fetches",
            count(tools, "web_fetch_requests").unwrap_or_default(),
        );
    }
    for name in ["speed", "service_tier", "inference_geo"] {
        if let Some(value) = usage.get(name).and_then(common::label) {
            normalized.dimensions.insert(name.into(), value);
        }
    }
    normalized.actual_service_tier = normalized.dimensions.get("service_tier").cloned();
    Some(normalized)
}

/// Turn fallback `iterations` or a refusal into billing attempts; see the
/// module documentation for which attempt is billable.
fn attach_attempts(usage: &mut NormalizedUsage, wire: &Value, model: &str, refused: bool) {
    let iterations = wire
        .get("iterations")
        .and_then(Value::as_array)
        .filter(|items| items.iter().any(|item| item["type"] == "fallback_message"));
    if let Some(iterations) = iterations {
        for (index, item) in iterations.iter().enumerate() {
            let Some(normalized) = from_usage(item) else {
                continue;
            };
            let produced = normalized.tokens.output_tokens.is_some_and(|n| n > 0);
            usage.attempts.push(UsageAttempt {
                model: item
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or(model)
                    .into(),
                billable: Some(produced || (!refused && index + 1 == iterations.len())),
                usage: Box::new(normalized),
                started_at_ms: None,
            });
        }
        return;
    }
    if refused && !model.is_empty() {
        usage.attempts = vec![UsageAttempt {
            model: model.to_owned(),
            billable: usage.tokens.output_tokens.map(|n| n > 0),
            usage: Box::new(usage.clone()),
            started_at_ms: None,
        }];
    }
}

/// The parts of a Messages response that metering reads; the content is
/// skipped without being materialized.
#[derive(Deserialize)]
struct Message {
    #[serde(default, deserialize_with = "common::string")]
    model: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    stop_reason: Option<String>,
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
}

/// Usage from a buffered Messages response.
pub(super) fn whole(body: &[u8]) -> Option<NormalizedUsage> {
    let message: Message = serde_json::from_slice(body).ok()?;
    let wire = message.usage?;
    let mut usage = from_usage(&wire)?;
    attach_attempts(
        &mut usage,
        &wire,
        message.model.as_deref().unwrap_or_default(),
        message.stop_reason.as_deref() == Some("refusal"),
    );
    Some(usage)
}

/// One stream event, reduced to what metering reads.
#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type", default, deserialize_with = "common::string")]
    kind: Option<String>,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
    #[serde(default)]
    delta: Option<Delta>,
    #[serde(default)]
    content_block: Option<Block>,
}

#[derive(Deserialize)]
struct Delta {
    #[serde(default, deserialize_with = "common::string")]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type", default, deserialize_with = "common::string")]
    kind: Option<String>,
    #[serde(default)]
    to: Option<Target>,
}

#[derive(Deserialize)]
struct Target {
    #[serde(default, deserialize_with = "common::string")]
    model: Option<String>,
}

/// The events that carry usage or name the model being billed.
const EVENTS: [&str; 3] = ["message_start", "message_delta", "content_block_start"];

/// Watches a Messages stream.
#[derive(Default)]
pub(super) struct Stream {
    start: Option<Map<String, Value>>,
    /// Every `message_delta` usage object seen, later keys replacing earlier.
    delta: Option<Map<String, Value>>,
    model: String,
    refused: bool,
}

impl Stream {
    pub(super) fn event(&mut self, name: Option<&str>, data: &str) {
        // The SSE event name is the cheapest filter, and a substring check on
        // the data the next: only an event that can bear usage is parsed.
        // Of the content blocks, only a fallback names a model.
        let named = name.is_none_or(|name| EVENTS.contains(&name));
        let bearing = ["\"message_start\"", "\"message_delta\"", "\"fallback\""]
            .iter()
            .any(|needle| data.contains(needle));
        if !named || !bearing {
            return;
        }
        let Ok(event) = serde_json::from_str::<Event>(data) else {
            return;
        };
        match event.kind.as_deref() {
            Some("message_start") if self.start.is_none() => {
                let Some(message) = event.message else {
                    return;
                };
                if let Some(model) = message.model {
                    self.model = model;
                }
                if let Some(Value::Object(usage)) = message.usage {
                    self.start = Some(usage);
                }
            }
            Some("content_block_start") => {
                let Some(block) = event.content_block else {
                    return;
                };
                if block.kind.as_deref() == Some("fallback")
                    && let Some(model) = block.to.and_then(|to| to.model)
                {
                    self.model = model;
                }
            }
            Some("message_delta") => {
                let Some(Value::Object(usage)) = event.usage else {
                    return;
                };
                if let Some(reason) = event.delta.and_then(|delta| delta.stop_reason) {
                    self.refused = reason == "refusal";
                }
                self.delta.get_or_insert_with(Map::new).extend(usage);
            }
            _ => {}
        }
    }

    pub(super) fn snapshot(&self) -> Option<NormalizedUsage> {
        let empty = Map::new();
        let start = self.start.as_ref().unwrap_or(&empty);
        let merged = match &self.delta {
            Some(delta) => merge(start, delta),
            None => start.clone(),
        };
        let merged = Value::Object(merged);
        let mut usage = from_usage(&merged)?;
        if self.delta.is_none() {
            usage.completeness = UsageCompleteness::Partial;
        }
        attach_attempts(&mut usage, &merged, &self.model, self.refused);
        Some(usage)
    }
}

/// `message_start`'s usage updated by the deltas; see the module docs.
fn merge(start: &Map<String, Value>, delta: &Map<String, Value>) -> Map<String, Value> {
    let mut merged = start.clone();
    for name in [
        "output_tokens",
        "output_tokens_details",
        "server_tool_use",
        "speed",
        "service_tier",
        "inference_geo",
        "iterations",
    ] {
        if let Some(value) = delta.get(name) {
            merged.insert(name.into(), value.clone());
        }
    }
    for name in ["input_tokens", "cache_read_input_tokens"] {
        if let Some(value) = delta.get(name).filter(|value| value.is_u64()) {
            merged.insert(name.into(), value.clone());
        }
    }
    let has_creation = |usage: &Map<String, Value>| {
        usage.get("cache_creation").is_some_and(Value::is_object)
            || usage
                .get("cache_creation_input_tokens")
                .is_some_and(Value::is_u64)
    };
    let delta_breakdown = delta.get("cache_creation").is_some_and(Value::is_object);
    if delta_breakdown || (!has_creation(start) && has_creation(delta)) {
        for name in ["cache_creation", "cache_creation_input_tokens"] {
            if let Some(value) = delta.get(name) {
                merged.insert(name.into(), value.clone());
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::super::tests::{chunked, feed_sse};
    use super::super::{UsageStreamEnd, whole as read_whole};
    use super::*;
    use crate::{Dialect, Operation};
    use serde_json::json;

    fn buffered(body: &Value) -> Option<NormalizedUsage> {
        read_whole(
            Operation::GenerateContent,
            Dialect::Claude,
            body.to_string().as_bytes(),
        )
    }

    fn streamed(wire: &str, split: usize, end: UsageStreamEnd) -> Option<NormalizedUsage> {
        feed_sse(
            Operation::StreamGenerateContent,
            Dialect::Claude,
            &chunked(wire, split),
            end,
        )
    }

    #[test]
    fn a_buffered_message_reads_every_standard_field() {
        let usage = buffered(&json!({
            "model": "claude-sonnet-4-6", "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "hello"}],
            "usage": {
                "input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 30,
                "cache_creation": {"ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3},
                "cache_creation_input_tokens": 5,
                "output_tokens_details": {"thinking_tokens": 1},
                "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
                "service_tier": "standard", "speed": "fast", "inference_geo": {"name": "us"}
            }
        }))
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(10), "already exclusive");
        assert_eq!(usage.tokens.output_tokens, Some(4));
        assert_eq!(usage.tokens.cached_input_tokens, Some(30));
        assert_eq!(usage.tokens.cache_creation_5m_tokens, Some(2));
        assert_eq!(usage.tokens.cache_creation_1h_tokens, Some(3));
        assert_eq!(usage.tokens.reasoning_tokens, Some(1));
        assert_eq!(usage.metrics["web_searches"], 2.into());
        assert_eq!(usage.metrics["web_fetches"], 1.into());
        assert_eq!(usage.dimensions["speed"], "fast");
        assert_eq!(usage.dimensions["inference_geo"], "us");
        assert_eq!(usage.actual_service_tier.as_deref(), Some("standard"));
        assert_eq!(usage.completeness, UsageCompleteness::Complete);
        assert!(
            usage.attempts.is_empty(),
            "an ordinary answer is one charge"
        );

        let flat = buffered(&json!({"usage": {"input_tokens": 1, "output_tokens": 1,
            "cache_creation_input_tokens": 7}}))
        .unwrap();
        assert_eq!(flat.tokens.cache_creation_5m_tokens, Some(7), "flat is 5m");
        assert_eq!(flat.tokens.cache_creation_1h_tokens, None);
    }

    #[test]
    fn fallback_iterations_and_refusals_become_attempts() {
        let usage = buffered(&json!({
            "model": "claude-opus-4-8", "stop_reason": "end_turn",
            "usage": {"input_tokens": 25, "output_tokens": 12, "iterations": [
                {"type": "fallback_message", "model": "claude-fable-5", "input_tokens": 25, "output_tokens": 0},
                {"type": "message", "input_tokens": 25, "output_tokens": 12}
            ]}
        }))
        .unwrap();
        assert_eq!(usage.attempts.len(), 2);
        assert_eq!(usage.attempts[0].model, "claude-fable-5");
        assert_eq!(usage.attempts[0].billable, Some(false));
        assert_eq!(usage.attempts[1].model, "claude-opus-4-8");
        assert_eq!(usage.attempts[1].billable, Some(true));

        let refused = buffered(&json!({"model": "claude-x", "stop_reason": "refusal",
            "usage": {"input_tokens": 9, "output_tokens": 0}}))
        .unwrap();
        assert_eq!(refused.attempts.len(), 1);
        assert_eq!(refused.attempts[0].billable, Some(false));
    }

    const STREAM: &str = concat!(
        "event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-fable-5\",\"content\":[],\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10,\"cache_creation\":{\"ephemeral_5m_input_tokens\":0,\"ephemeral_1h_input_tokens\":20}}}}\r\n\r\n",
        "event: ping\ndata: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"fallback\",\"from\":{\"model\":\"claude-fable-5\"},\"to\":{\"model\":\"claude-opus-4-8\"}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"message_delta usage\"}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12,\"cache_creation_input_tokens\":20,\"output_tokens_details\":{\"thinking_tokens\":4},\"iterations\":[{\"type\":\"fallback_message\",\"model\":\"claude-fable-5\",\"input_tokens\":25,\"output_tokens\":0},{\"type\":\"message\",\"input_tokens\":25,\"output_tokens\":12}]}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );

    #[test]
    fn a_stream_merges_start_and_delta_in_any_chunking() {
        let expected = streamed(STREAM, usize::MAX, UsageStreamEnd::Complete).unwrap();
        assert_eq!(expected.tokens.input_tokens, Some(25));
        assert_eq!(expected.tokens.output_tokens, Some(12));
        assert_eq!(expected.tokens.cached_input_tokens, Some(10));
        assert_eq!(
            expected.tokens.cache_creation_5m_tokens,
            Some(0),
            "message_start's breakdown survives the delta's flat restatement"
        );
        assert_eq!(expected.tokens.cache_creation_1h_tokens, Some(20));
        assert_eq!(expected.tokens.reasoning_tokens, Some(4));
        assert_eq!(expected.completeness, UsageCompleteness::Complete);
        assert_eq!(expected.attempts.len(), 2);
        assert_eq!(
            expected.attempts[1].model, "claude-opus-4-8",
            "fallback target"
        );
        for split in [1, 2, 3, 7, 37, 101] {
            assert_eq!(
                streamed(STREAM, split, UsageStreamEnd::Complete).as_ref(),
                Some(&expected),
                "split {split}"
            );
        }
    }

    #[test]
    fn a_delta_breakdown_replaces_the_start_buckets() {
        for (five, hour) in [(0, 20), (7, 13), (20, 0)] {
            let start = json!({"type":"message_start","message":{"usage":{"input_tokens":10,"output_tokens":0,
                "cache_creation":{"ephemeral_5m_input_tokens":five,"ephemeral_1h_input_tokens":hour}}}});
            let delta = json!({"type":"message_delta","usage":{"output_tokens":5,"cache_creation_input_tokens":20}});
            let wire = format!("data: {start}\n\ndata: {delta}\n\n");
            let merged = streamed(&wire, 11, UsageStreamEnd::Complete).unwrap();
            assert_eq!(merged.tokens.cache_creation_5m_tokens, Some(five));
            assert_eq!(merged.tokens.cache_creation_1h_tokens, Some(hour));
            let update = json!({"type":"message_delta","usage":{"output_tokens":6,
                "cache_creation":{"ephemeral_5m_input_tokens":2,"ephemeral_1h_input_tokens":22}}});
            let merged = streamed(
                &format!("{wire}data: {update}\n\n"),
                13,
                UsageStreamEnd::Complete,
            )
            .unwrap();
            assert_eq!(merged.tokens.cache_creation_5m_tokens, Some(2));
            assert_eq!(merged.tokens.cache_creation_1h_tokens, Some(22));
            assert_eq!(merged.tokens.output_tokens, Some(6));
        }
    }

    #[test]
    fn a_stream_cut_before_its_delta_is_partial() {
        let cut = STREAM.split("event: message_delta").next().unwrap();
        let usage = streamed(cut, 5, UsageStreamEnd::Interrupted).unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(25));
        assert_eq!(usage.tokens.output_tokens, Some(1), "provisional");
        assert_eq!(usage.completeness, UsageCompleteness::Partial);

        let whole = streamed(STREAM, 5, UsageStreamEnd::Interrupted).unwrap();
        assert_eq!(
            whole.completeness,
            UsageCompleteness::Partial,
            "an interrupted stream is never complete"
        );
    }

    #[test]
    fn events_without_usage_and_malformed_input_report_nothing() {
        let quiet = "event: ping\ndata: {\"type\":\"ping\"}\n\nevent: message_stop\ndata: {}\n\n";
        assert!(streamed(quiet, 3, UsageStreamEnd::Complete).is_none());
        assert!(
            streamed(
                "data: {\"type\":\"message_start\",\"message\":7}\n\n",
                4,
                UsageStreamEnd::Complete
            )
            .is_none()
        );
        assert!(streamed("\u{0}\u{1}garbage\n\n\n", 2, UsageStreamEnd::Complete).is_none());
        for body in [
            &b"not json"[..],
            b"{\"usage\":\"none\"}",
            b"{\"usage\":{\"input_tokens\":-1,\"output_tokens\":\"x\"}}",
            b"[]",
            b"",
            b"{\"model\":5,\"usage\":{\"input_tokens\":1,\"output_tokens\":2}",
        ] {
            assert!(
                read_whole(Operation::GenerateContent, Dialect::Claude, body).is_none(),
                "{}",
                String::from_utf8_lossy(body)
            );
        }
        let odd = read_whole(
            Operation::GenerateContent,
            Dialect::Claude,
            br#"{"model":5,"stop_reason":[1],"usage":{"input_tokens":1,"output_tokens":2}}"#,
        )
        .unwrap();
        assert_eq!(
            odd.tokens.output_tokens,
            Some(2),
            "odd qualifiers are skipped"
        );
    }
}
