//! Provider rule processing (§6.1): applies §8-B2 rule-set mutations to the
//! provider-native request, after transform and before the channel. Fixed kind
//! order: system_text → cache_breakpoint → rewrite → sanitize → header.

mod apply_content;
mod apply_generic;
mod compile;

pub use compile::{
    CacheBreakpointCfg, CompiledRule, HeaderMode, RewriteAction, RuleConfig, TextPosition,
    compile_rules, order_for_apply,
};

use bytes::Bytes;
use http::HeaderMap;

use crate::protocol::{ContentGenerationKind, OperationKey};

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

    // Phase 1 — JSON-value mutations (system_text / cache_breakpoint / rewrite),
    // already rank-ordered.
    let mut value: Option<serde_json::Value> = None;
    for rule in applicable.iter().filter(|r| r.config.mutates_value()) {
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

    // Phase 2 — sanitize (regex over the serialized body).
    for rule in &applicable {
        if let RuleConfig::Sanitize { regex, replacement } = &rule.config {
            body = apply_generic::sanitize(body, regex, replacement);
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
