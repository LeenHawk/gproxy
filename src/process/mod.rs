//! Provider rule processing (§6.1): applies §8-B2 rule-set mutations to the
//! provider-native request/response. Request rules run after transform and
//! before the channel. Fixed kind order: system_text → cache_breakpoint →
//! rewrite → transform → header.

mod apply_content;
mod apply_generic;
mod compile;

pub use compile::{
    CacheBreakpointCfg, CompiledRule, HeaderMode, RewriteAction, RuleConfig, TextPosition,
    TransformAction, TransformCfg, TransformLocate, TransformPhase, compile_rules, order_for_apply,
};

use bytes::Bytes;
use http::HeaderMap;
use serde_json::Value;

use crate::http::client::ByteStreamDecoder;
use crate::protocol::{ContentGenerationKind, OperationKey};
use crate::transform::common::sse::{SseDecoder, SseFrame};

/// Apply every matching rule to the provider-native request. `rules` must be
/// pre-ordered by [`order_for_apply`]. The JSON body parses at most once and
/// re-serializes at most once; rules that cannot apply warn and skip — bad
/// rule config must never break traffic.
pub fn apply(
    rules: &[CompiledRule],
    op: OperationKey,
    kind: Option<ContentGenerationKind>,
    model: &str,
    headers: &mut HeaderMap,
    body: Bytes,
) -> Bytes {
    let applicable: Vec<&CompiledRule> = rules.iter().filter(|r| r.matches(op, model)).collect();
    if applicable.is_empty() {
        return body;
    }

    // Phase 1 — JSON-value mutations (system_text / cache_breakpoint / rewrite
    // / transform.path), already rank-ordered.
    let mut value: Option<serde_json::Value> = None;
    for rule in applicable
        .iter()
        .filter(|r| r.config.mutates_request_value())
    {
        if value.is_none() {
            match serde_json::from_slice(&body) {
                Ok(v) => value = Some(v),
                Err(e) => {
                    tracing::warn!(error = %e, "process: body is not JSON; value rules skipped");
                    break;
                }
            }
        }
        let Some(v) = value.as_mut() else { break };
        match &rule.config {
            RuleConfig::SystemText { text, position } => {
                apply_content::system_text(v, kind, text, *position)
            }
            RuleConfig::CacheBreakpoint(cfg) => apply_content::cache_breakpoint(v, kind, cfg),
            RuleConfig::Rewrite {
                path,
                action,
                value_json,
            } => apply_generic::rewrite(v, path, *action, value_json.as_ref()),
            RuleConfig::Transform(cfg) => apply_generic::transform_value(v, cfg),
            _ => {}
        }
    }
    let mut body = match value {
        Some(v) => match serde_json::to_vec(&v) {
            Ok(b) => Bytes::from(b),
            Err(e) => {
                tracing::warn!(error = %e, "process: re-serialize failed; original body kept");
                body
            }
        },
        None => body,
    };

    // Phase 2 — text transforms (regex over the serialized body).
    for rule in applicable
        .iter()
        .filter(|r| r.config.mutates_request_text())
    {
        if let RuleConfig::Transform(cfg) = &rule.config {
            body = apply_generic::transform_text(body, cfg);
        }
    }

    // Phase 3 — header rules.
    for rule in &applicable {
        if let RuleConfig::Header { name, value, mode } = &rule.config {
            apply_generic::header(headers, name, value, *mode);
        }
    }
    body
}

pub fn apply_response(
    rules: &[CompiledRule],
    op: OperationKey,
    kind: Option<ContentGenerationKind>,
    model: &str,
    body: Bytes,
) -> Bytes {
    let applicable: Vec<&CompiledRule> = rules.iter().filter(|r| r.matches(op, model)).collect();
    if applicable.is_empty() {
        return body;
    }

    let mut value: Option<Value> = None;
    for rule in applicable
        .iter()
        .filter(|r| r.config.mutates_response_value())
    {
        if value.is_none() {
            match serde_json::from_slice(&body) {
                Ok(v) => value = Some(v),
                Err(e) => {
                    tracing::warn!(error = %e, "process: response body is not JSON; value rules skipped");
                    break;
                }
            }
        }
        let Some(v) = value.as_mut() else { break };
        if let RuleConfig::Transform(cfg) = &rule.config {
            apply_generic::transform_value(v, cfg);
        }
    }

    let mut body = match value {
        Some(v) => match serde_json::to_vec(&v) {
            Ok(b) => Bytes::from(b),
            Err(e) => {
                tracing::warn!(error = %e, "process: response re-serialize failed; original body kept");
                body
            }
        },
        None => body,
    };

    for rule in applicable
        .iter()
        .filter(|r| r.config.mutates_response_text())
    {
        if let RuleConfig::Transform(cfg) = &rule.config {
            body = apply_generic::transform_text(body, cfg);
        }
    }

    let _ = kind;
    body
}

pub fn response_stream_decoder(
    rules: &[CompiledRule],
    op: OperationKey,
    kind: Option<ContentGenerationKind>,
    model: &str,
) -> Option<Box<dyn ByteStreamDecoder>> {
    let rules: Vec<CompiledRule> = rules
        .iter()
        .filter(|rule| {
            rule.matches(op, model)
                && (rule.config.mutates_response_value() || rule.config.mutates_response_text())
        })
        .cloned()
        .collect();
    if rules.is_empty() {
        return None;
    }
    Some(Box::new(ResponseRuleStreamDecoder {
        decoder: SseDecoder::new(),
        rules,
        op,
        kind,
        model: model.to_owned(),
    }))
}

struct ResponseRuleStreamDecoder {
    decoder: SseDecoder,
    rules: Vec<CompiledRule>,
    op: OperationKey,
    kind: Option<ContentGenerationKind>,
    model: String,
}

impl ByteStreamDecoder for ResponseRuleStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for frame in self.decoder.push(chunk) {
            out.extend_from_slice(self.apply_frame(frame).encode().as_bytes());
        }
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(frame) = self.decoder.finish() {
            out.extend_from_slice(self.apply_frame(frame).encode().as_bytes());
        }
        out
    }
}

impl ResponseRuleStreamDecoder {
    fn apply_frame(&self, frame: SseFrame) -> SseFrame {
        if frame.data.trim() == "[DONE]" {
            return frame;
        }
        let body = apply_response(
            &self.rules,
            self.op,
            self.kind,
            &self.model,
            Bytes::from(frame.data.clone()),
        );
        match String::from_utf8(body.to_vec()) {
            Ok(data) => SseFrame {
                event: frame.event,
                data,
            },
            Err(_) => frame,
        }
    }
}
