//! Usage extraction from JSON, HTTP SSE and WebSocket events.

use super::Codex;
use super::common::invalid_response;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame,
    UsageObserver, UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_protocol::Operation;
use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
use gproxy_protocol::connection::{StreamFraming, WsFrame};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Bounds for watching a Responses SSE stream; the host enforces the real
/// transfer limits, this only keeps the observer's buffers finite.
pub(super) const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 4 * 1024 * 1024,
    max_value_bytes: 4 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 4 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};
/// Responses `usage` -> normalized. Reported input counts include cached
/// reads and writes; the normalized input excludes both.
fn usage_from_value(usage: &Value) -> Option<NormalizedUsage> {
    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens").and_then(Value::as_u64);
    let cached = usage
        .pointer("/input_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_token_details/cached_tokens"))
        .and_then(Value::as_u64);
    let cache_write = usage
        .pointer("/input_tokens_details/cache_write_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.pointer("/input_token_details/cache_write_tokens").and_then(Value::as_u64));
    let reasoning = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage::default();
    normalized.tokens.input_tokens = Some(input.saturating_sub(cached.unwrap_or(0)).saturating_sub(cache_write.unwrap_or(0)));
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.cache_creation_30m_tokens = cache_write;
    normalized.tokens.reasoning_tokens = reasoning;
    for (prefix, details) in [
        ("input", "input_token_details"),
        ("output", "output_token_details"),
    ] {
        let details = usage
            .get(details)
            .or_else(|| usage.get(format!("{prefix}_tokens_details")));
        if let Some(details) = details {
            for modality in ["audio", "text", "image"] {
                if let Some(count) = details
                    .get(format!("{modality}_tokens"))
                    .and_then(Value::as_u64)
                {
                    normalized
                        .metrics
                        .insert(format!("{modality}_{prefix}_tokens"), count.into());
                }
                if prefix == "input"
                    && let Some(count) = details
                        .pointer(&format!("/cached_tokens_details/{modality}_tokens"))
                        .and_then(Value::as_u64)
                {
                    normalized
                        .metrics
                        .insert(format!("cached_{modality}_input_tokens"), count.into());
                }
            }
        }
    }
    if !normalized.metrics.is_empty() {
        normalized
            .dimensions
            .insert("token_modalities_in_totals".into(), "true".into());
    }
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

fn usage_from_response_json(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"))?;
    usage_from_value(usage)
}

impl UsageExtractor for Codex {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage_from_response_json(ctx.response.body))
    }
}

/// Watches a Responses stream for its terminal event's `usage`, over SSE
/// data frames or WebSocket text messages.
struct ResponsesUsageObserver {
    sse: Option<SseDecoder>,
    usage: Option<NormalizedUsage>,
    operation: Operation,
    responses: BTreeMap<String, NormalizedUsage>,
    active: BTreeSet<String>,
}

impl ResponsesUsageObserver {
    fn see_event(&mut self, data: &str) {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let realtime = self.operation == Operation::ConnectRealtime;
        if realtime && kind == "response.created" {
            if let Some(id) = value.pointer("/response/id").and_then(Value::as_str) {
                self.active.insert(id.into());
            }
            return;
        }
        let image = matches!(
            self.operation,
            Operation::CreateImage | Operation::EditImage
        );
        let response =
            if image && matches!(kind, "image_generation.completed" | "image_edit.completed") {
                &value
            } else if matches!(
                kind,
                "response.completed" | "response.incomplete" | "response.failed" | "response.done"
            ) {
                value.get("response").unwrap_or(&Value::Null)
            } else {
                return;
            };
        let id = response
            .get("id")
            .or_else(|| response.get("generation_id"))
            .and_then(Value::as_str);
        let Some(mut usage) = response.get("usage").and_then(usage_from_value) else {
            if realtime && let Some(id) = id {
                self.active.insert(id.into());
            }
            return;
        };
        usage.actual_service_tier = response
            .get("service_tier")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if image {
            usage.metrics.insert("image_outputs".into(), 1.into());
            for field in ["size", "quality", "output_format", "background"] {
                if let Some(v) = response.get(field).and_then(Value::as_str) {
                    usage.dimensions.insert(field.into(), v.into());
                }
            }
        }
        if realtime {
            // Native response IDs make repeated terminal events replacements,
            // not additional charges. Never invent an ID for billable usage.
            let Some(id) = id else {
                return;
            };
            self.active.remove(id);
            self.responses.insert(id.into(), usage);
            let mut aggregate = NormalizedUsage::aggregate(self.responses.values());
            aggregate.responses = self
                .responses
                .iter()
                .map(|(id, usage)| crate::channel::ResponseUsage {
                    id: id.clone(),
                    usage: Box::new(usage.clone()),
                })
                .collect();
            self.usage = Some(aggregate);
        } else {
            self.usage = Some(usage);
        }
    }
}

impl UsageObserver for ResponsesUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        match frame {
            UsageFrame::HttpChunk(chunk) => {
                let Some(decoder) = self.sse.as_mut() else {
                    return Ok(());
                };
                let frames = decoder
                    .push(chunk)
                    .map_err(|e| invalid_response(e.to_string()))?;
                for frame in frames {
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
        if !self.active.is_empty()
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

impl UsageStream for Codex {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        let sse = match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => {
                let mut limits = SSE_LIMITS;
                if matches!(
                    context.operation.operation,
                    Operation::CreateImage | Operation::EditImage
                ) {
                    // Base64 image events are larger than text Responses frames.
                    limits.max_buffer_bytes = 64 * 1024 * 1024;
                    limits.max_value_bytes = 64 * 1024 * 1024;
                    limits.max_line_bytes = 64 * 1024 * 1024;
                }
                Some(SseDecoder::new(limits))
            }
            UsageTransport::Http { .. } => {
                return Err(ChannelError::InvalidResponse(
                    "Responses streams are SSE".into(),
                ));
            }
            UsageTransport::WebSocket => None,
        };
        Ok(Box::new(ResponsesUsageObserver {
            sse,
            usage: None,
            operation: context.operation.operation,
            responses: BTreeMap::new(),
            active: BTreeSet::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn observer(operation: Operation) -> ResponsesUsageObserver {
        ResponsesUsageObserver {
            sse: None,
            operation,
            usage: None,
            responses: BTreeMap::new(),
            active: BTreeSet::new(),
        }
    }
    #[test]
    fn cache_writes_are_30m_and_not_ordinary_input_in_json_sse_and_websocket() {
        for details in ["input_tokens_details", "input_token_details"] {
            let usage = json!({"input_tokens":100,"output_tokens":20,
                details:{"cached_tokens":40,"cache_write_tokens":3}});
            let body = json!({"usage":usage}).to_string();
            let buffered = usage_from_response_json(body.as_bytes()).unwrap();
            assert_eq!(buffered.tokens.input_tokens, Some(57));
            assert_eq!(buffered.tokens.cached_input_tokens, Some(40));
            assert_eq!(buffered.tokens.cache_creation_30m_tokens, Some(3));
            let event = json!({"type":"response.completed","response":{"id":"r1","usage":usage}}).to_string();
            let mut sse = observer(Operation::StreamGenerateContent);
            sse.sse = Some(SseDecoder::new(SSE_LIMITS));
            for chunk in format!("data: {event}\n\n").as_bytes().chunks(7) {
                sse.observe(UsageFrame::HttpChunk(chunk)).unwrap();
            }
            assert_eq!(sse.snapshot().unwrap().tokens, buffered.tokens);
            let mut ws = observer(Operation::ConnectRealtime);
            let frame = WsFrame::Text(event.into());
            ws.observe(UsageFrame::WebSocket(&frame)).unwrap();
            ws.observe(UsageFrame::WebSocket(&frame)).unwrap();
            assert_eq!(ws.snapshot().unwrap().tokens, buffered.tokens, "repeated terminal events are not double counted");
        }
        let zero = usage_from_value(&json!({"input_tokens":10,"output_tokens":2,
            "input_tokens_details":{"cache_write_tokens":0}})).unwrap();
        assert_eq!(zero.tokens.cache_creation_30m_tokens, Some(0));
        let absent = usage_from_value(&json!({"input_tokens":10,"output_tokens":2})).unwrap();
        assert_eq!(absent.tokens.cache_creation_30m_tokens, None);
    }

    #[test]
    fn realtime_accumulates_unique_responses_and_tracks_audio_and_interruption() {
        let mut observer = observer(Operation::ConnectRealtime);
        let event=json!({"type":"response.done","response":{"id":"r1","status":"completed","usage":{
            "input_tokens":100,"output_tokens":30,"input_token_details":{"cached_tokens":20,"audio_tokens":70,"text_tokens":30,"cached_tokens_details":{"audio_tokens":15,"text_tokens":5}},
            "output_token_details":{"audio_tokens":25,"text_tokens":5}
        }}}).to_string();
        observer.see_event(&event);
        observer.see_event(&event);
        observer.see_event(&json!({"type":"response.done","response":{"id":"r2","usage":{"input_tokens":10,"output_tokens":4}}}).to_string());
        let usage = observer.snapshot().unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(90));
        assert_eq!(usage.tokens.output_tokens, Some(34));
        assert_eq!(usage.tokens.cached_input_tokens, Some(20));
        assert_eq!(usage.metrics["audio_input_tokens"], 70.into());
        assert_eq!(usage.metrics["cached_audio_input_tokens"], 15.into());
        assert_eq!(usage.metrics["audio_output_tokens"], 25.into());
        observer.see_event(&json!({"type":"response.created","response":{"id":"r3"}}).to_string());
        assert_eq!(
            observer.snapshot().unwrap().completeness,
            UsageCompleteness::Partial
        );
        let usage = Box::new(observer)
            .finish(UsageStreamEnd::Interrupted)
            .unwrap()
            .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(90));
        assert_eq!(usage.completeness, UsageCompleteness::Partial);
    }
    #[test]
    fn image_terminal_usage_is_counted_once_and_partial_images_are_not_outputs() {
        for (operation, event) in [
            (Operation::CreateImage, "image_generation.completed"),
            (Operation::EditImage, "image_edit.completed"),
        ] {
            let mut observer = observer(operation);
            observer.see_event(&json!({"type":"image_generation.partial_image","usage":{"input_tokens":10,"output_tokens":5}}).to_string());
            assert!(observer.snapshot().is_none());
            let event=json!({"type":event,"generation_id":"g1","size":"1024x1024","quality":"high","output_format":"png","usage":{"input_tokens":20,"output_tokens":7,"input_tokens_details":{"text_tokens":5,"image_tokens":15},"output_tokens_details":{"image_tokens":7}}}).to_string();
            observer.see_event(&event);
            observer.see_event(&event);
            let usage = observer.snapshot().unwrap();
            assert_eq!(usage.tokens.input_tokens, Some(20));
            assert_eq!(usage.metrics["image_outputs"], 1.into());
            assert_eq!(usage.metrics["image_input_tokens"], 15.into());
            assert_eq!(usage.metrics["image_output_tokens"], 7.into());
            assert_eq!(usage.dimensions["quality"], "high");
        }
    }
}
