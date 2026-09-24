//! Usage out of the three compatible wire shapes, buffered and streamed.
//!
//! Chat Completions reports `prompt_tokens`/`completion_tokens` with cache
//! reads *included* in the prompt count; Responses reports
//! `input_tokens`/`output_tokens`, again with cache reads included; Claude
//! Messages reports `input_tokens` already exclusive of its two cache
//! counters. `TokenUsage::input_tokens` is the exclusive number everywhere,
//! so the first two shapes subtract and the third does not.
//!
//! Each channel passes an `Enrich` hook that reads its vendor's own fields
//! (a reported cost, a cache counter under a private name) off the response
//! root and the usage object. A `None` return means the response reported no
//! usage, never zero consumption.

use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageFrame, UsageObserver, UsageStreamEnd,
    UsageTransport,
};
use gproxy_protocol::Dialect;
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::connection::WsFrame;
use serde_json::Value;

/// Bounds for watching a stream. The host enforces the real transfer limits;
/// this only keeps the observer's own buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: u64::MAX,
    max_value_bytes: u64::MAX,
    max_body_bytes: u64::MAX,
    max_line_bytes: u64::MAX,
    max_part_bytes: 0,
    max_parts: 0,
};

/// Vendor-specific fields, read from the response (or event) root and from
/// the usage object it contains.
pub(crate) type Enrich = fn(root: &Value, usage: &Value, into: &mut NormalizedUsage);

pub(crate) fn u64_at(value: &Value, pointer: &str) -> Option<u64> {
    value.pointer(pointer).and_then(Value::as_u64)
}

fn tokens(dialect: Dialect, usage: &Value) -> Option<NormalizedUsage> {
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    match dialect {
        Dialect::Claude => {
            // Claude's input count already excludes both cache counters.
            let input = u64_at(usage, "/input_tokens");
            let output = u64_at(usage, "/output_tokens");
            input?;
            normalized.tokens.input_tokens = input;
            normalized.tokens.output_tokens = output;
            normalized.tokens.cached_input_tokens = u64_at(usage, "/cache_read_input_tokens");
            let five_minute = u64_at(usage, "/cache_creation/ephemeral_5m_input_tokens");
            let one_hour = u64_at(usage, "/cache_creation/ephemeral_1h_input_tokens");
            match (five_minute, one_hour) {
                (None, None) => {
                    normalized.tokens.cache_creation_5m_tokens =
                        u64_at(usage, "/cache_creation_input_tokens");
                }
                _ => {
                    normalized.tokens.cache_creation_5m_tokens = five_minute;
                    normalized.tokens.cache_creation_1h_tokens = one_hour;
                }
            }
        }
        Dialect::OpenAiChat => {
            let prompt = u64_at(usage, "/prompt_tokens")?;
            let cached = u64_at(usage, "/prompt_tokens_details/cached_tokens");
            normalized.tokens.input_tokens = Some(prompt.saturating_sub(cached.unwrap_or(0)));
            normalized.tokens.output_tokens = u64_at(usage, "/completion_tokens");
            normalized.tokens.cached_input_tokens = cached;
            normalized.tokens.reasoning_tokens =
                u64_at(usage, "/completion_tokens_details/reasoning_tokens");
        }
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => {
            let input = u64_at(usage, "/input_tokens")?;
            let cached = u64_at(usage, "/input_tokens_details/cached_tokens");
            normalized.tokens.input_tokens = Some(input.saturating_sub(cached.unwrap_or(0)));
            normalized.tokens.output_tokens = u64_at(usage, "/output_tokens");
            normalized.tokens.cached_input_tokens = cached;
            normalized.tokens.reasoning_tokens =
                u64_at(usage, "/output_tokens_details/reasoning_tokens");
        }
        // No channel in this fleet speaks Gemini, and Gemini does not report a
        // `usage` object at all — its counts are `usageMetadata` with
        // `promptTokenCount`/`candidatesTokenCount`, which
        // `shared::vendor_usage::gemini` reads. This arm used to sit with the
        // Responses shapes above, where a native Gemini body would have
        // metered nothing and said nothing about it; a fleet channel that
        // grows a Gemini surface should take the reading it needs rather than
        // inherit OpenAI's field names.
        Dialect::Gemini => return None,
    }
    Some(normalized)
}

/// The usage object of a buffered response, wherever the shape keeps it.
fn locate(root: &Value) -> Option<&Value> {
    root.get("usage")
        .filter(|usage| !usage.is_null())
        .or_else(|| root.pointer("/response/usage"))
}

/// Usage from a buffered JSON response. The response root is handed to
/// `enrich` as well, because vendors report byok flags and job costs beside
/// the usage object rather than inside it.
pub(crate) fn from_body(dialect: Dialect, body: &[u8], enrich: Enrich) -> Option<NormalizedUsage> {
    let root: Value = serde_json::from_slice(body).ok()?;
    from_root(dialect, &root, enrich)
}

pub(crate) fn from_root(dialect: Dialect, root: &Value, enrich: Enrich) -> Option<NormalizedUsage> {
    let usage = locate(root);
    let measured = usage.and_then(|usage| tokens(dialect, usage));
    let reported = measured.is_some();
    let mut normalized = measured.unwrap_or_default();
    let empty = Value::Null;
    // The hook sees the whole normalized value: a vendor that keeps a cache
    // counter under its own name corrects a token field, not just a metric.
    enrich(root, usage.unwrap_or(&empty), &mut normalized);
    if reported {
        return Some(normalized);
    }
    if normalized == NormalizedUsage::default() {
        return None;
    }
    // An upstream that reported only a price still reported something.
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

/// Replace the fields `later` establishes, keeping what it leaves unknown.
/// Claude splits one response's usage across `message_start` and
/// `message_delta`; the delta is authoritative for what it names.
fn merge(into: &mut NormalizedUsage, later: NormalizedUsage) {
    fn replace(target: &mut Option<u64>, value: Option<u64>) {
        if value.is_some() {
            *target = value;
        }
    }
    replace(&mut into.tokens.input_tokens, later.tokens.input_tokens);
    replace(&mut into.tokens.output_tokens, later.tokens.output_tokens);
    replace(
        &mut into.tokens.cached_input_tokens,
        later.tokens.cached_input_tokens,
    );
    replace(
        &mut into.tokens.cache_creation_5m_tokens,
        later.tokens.cache_creation_5m_tokens,
    );
    replace(
        &mut into.tokens.cache_creation_1h_tokens,
        later.tokens.cache_creation_1h_tokens,
    );
    replace(
        &mut into.tokens.reasoning_tokens,
        later.tokens.reasoning_tokens,
    );
    into.metrics.extend(later.metrics);
    into.dimensions.extend(later.dimensions);
    into.completeness = later.completeness;
}

/// Watches a stream of the dialect's own events for the usage it ends with.
struct CompatibleObserver {
    sse: Option<SseDecoder>,
    dialect: Dialect,
    enrich: Enrich,
    usage: Option<NormalizedUsage>,
    /// Claude's `message_start` is a beginning, not a settlement.
    awaiting_delta: bool,
}

impl CompatibleObserver {
    fn see_event(&mut self, data: &str) {
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        if self.dialect == Dialect::Claude {
            let (pointer, terminal) = match kind {
                "message_start" => ("/message/usage", false),
                "message_delta" => ("/usage", true),
                _ => return,
            };
            let Some(usage) = event.pointer(pointer) else {
                return;
            };
            let Some(mut parsed) = tokens(Dialect::Claude, usage) else {
                return;
            };
            if !terminal {
                parsed.completeness = UsageCompleteness::Partial;
            }
            (self.enrich)(&event, usage, &mut parsed);
            self.awaiting_delta = !terminal;
            match &mut self.usage {
                Some(existing) => merge(existing, parsed),
                None => self.usage = Some(parsed),
            }
            return;
        }
        // Responses names its terminal event; Chat Completions puts a
        // non-null usage on the last chunk and nowhere else.
        if kind.starts_with("response.")
            && !matches!(
                kind,
                "response.completed" | "response.incomplete" | "response.failed" | "response.done"
            )
        {
            return;
        }
        if let Some(parsed) = from_root(self.dialect, &event, self.enrich) {
            self.usage = Some(parsed);
        }
    }
}

impl UsageObserver for CompatibleObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        match frame {
            UsageFrame::HttpChunk(chunk) => {
                let Some(decoder) = self.sse.as_mut() else {
                    return Ok(());
                };
                for frame in decoder
                    .push(chunk)
                    .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?
                {
                    if let SseFrame::Event(event) = frame {
                        self.see_event(&event.data);
                    }
                }
            }
            UsageFrame::WebSocket(WsFrame::Text(text)) => self.see_event(text),
            UsageFrame::WebSocket(_) => {}
        }
        Ok(())
    }

    fn snapshot(&self) -> Option<NormalizedUsage> {
        let mut usage = self.usage.clone();
        if self.awaiting_delta
            && let Some(usage) = &mut usage
        {
            usage.completeness = UsageCompleteness::Partial;
        }
        usage
    }

    fn finish(
        self: Box<Self>,
        end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        let mut usage = self.snapshot();
        if end == UsageStreamEnd::Interrupted
            && let Some(usage) = &mut usage
        {
            usage.completeness = UsageCompleteness::Partial;
        }
        Ok(usage)
    }
}

/// An observer for one response. A transport that is not SSE (a buffered
/// reply, an audio download) reports nothing rather than failing the call.
pub(crate) fn observer(
    dialect: Dialect,
    transport: UsageTransport,
    enrich: Enrich,
) -> Box<dyn UsageObserver> {
    let sse = matches!(
        transport,
        UsageTransport::Http {
            framing: Some(gproxy_protocol::connection::StreamFraming::Sse)
        }
    )
    .then(|| SseDecoder::new(SSE_LIMITS));
    Box::new(CompatibleObserver {
        sse,
        dialect,
        enrich,
        usage: None,
        awaiting_delta: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A channel with nothing of its own to add.
    fn no_enrich(_: &Value, _: &Value, _: &mut NormalizedUsage) {}

    #[test]
    fn each_shape_reports_exclusive_input_tokens() {
        let chat = json!({"usage": {"prompt_tokens": 100, "completion_tokens": 20,
            "prompt_tokens_details": {"cached_tokens": 30},
            "completion_tokens_details": {"reasoning_tokens": 5}}});
        let usage = from_root(Dialect::OpenAiChat, &chat, no_enrich).unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(70));
        assert_eq!(usage.tokens.cached_input_tokens, Some(30));
        assert_eq!(usage.tokens.reasoning_tokens, Some(5));

        let responses = json!({"response": {"usage": {"input_tokens": 100, "output_tokens": 20,
            "input_tokens_details": {"cached_tokens": 40}}}});
        let usage = from_root(Dialect::OpenAi, &responses, no_enrich).unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(60));
        assert_eq!(usage.tokens.cached_input_tokens, Some(40));

        let claude = json!({"usage": {"input_tokens": 100, "output_tokens": 20,
            "cache_read_input_tokens": 40, "cache_creation_input_tokens": 7}});
        let usage = from_root(Dialect::Claude, &claude, no_enrich).unwrap();
        assert_eq!(
            usage.tokens.input_tokens,
            Some(100),
            "Claude already excludes its cache counters"
        );
        assert_eq!(usage.tokens.cache_creation_5m_tokens, Some(7));

        assert!(from_root(Dialect::OpenAiChat, &json!({"usage": null}), no_enrich).is_none());
    }

    #[test]
    fn claude_streams_settle_on_the_delta_and_stay_partial_before_it() {
        let mut observer = observer(
            Dialect::Claude,
            UsageTransport::Http {
                framing: Some(gproxy_protocol::connection::StreamFraming::Sse),
            },
            no_enrich,
        );
        let start = json!({"type": "message_start", "message": {"usage":
            {"input_tokens": 10, "output_tokens": 1, "cache_read_input_tokens": 4}}});
        observer
            .observe(UsageFrame::HttpChunk(
                format!("data: {start}\n\n").as_bytes(),
            ))
            .unwrap();
        let partial = observer.snapshot().unwrap();
        assert_eq!(partial.tokens.input_tokens, Some(10));
        assert_eq!(partial.completeness, UsageCompleteness::Partial);

        let delta =
            json!({"type": "message_delta", "usage": {"input_tokens": 10, "output_tokens": 55}});
        observer
            .observe(UsageFrame::HttpChunk(
                format!("data: {delta}\n\n").as_bytes(),
            ))
            .unwrap();
        let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
        assert_eq!(usage.tokens.output_tokens, Some(55));
        assert_eq!(usage.tokens.cached_input_tokens, Some(4), "kept from start");
        assert_eq!(usage.completeness, UsageCompleteness::Complete);
    }
}
