//! Always-on structural hygiene and cache marker handling for Claude bodies.
//!
//! String content is canonicalized to block arrays before manual marker
//! selection or sanitization. Sanitization migrates orphaned `cache_control`
//! markers to a surviving cacheable block where possible.

#[path = "claude_cache_control/canonicalize.rs"]
mod canonicalize;
#[path = "claude_cache_control/helpers.rs"]
mod helpers;
#[path = "claude_cache_control/manual.rs"]
mod manual;
#[path = "claude_cache_control/sanitize.rs"]
mod sanitize;
#[path = "claude_cache_control/schema.rs"]
mod schema;
#[path = "claude_cache_control/selection.rs"]
mod selection;

use serde_json::{Map, Value};

const MAX_BREAKPOINTS: usize = 4;

pub(super) fn canonicalize_claude_body(body: &mut Value) {
    canonicalize::body(body);
}

/// Apply a provider rule to a Claude body. Message indexes address one flat
/// sequence of cacheable blocks across all messages, in prompt order.
pub fn apply_manual_cache_breakpoint(
    body: &mut Value,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    manual::apply(body, target, index, ttl)
}

pub(super) fn existing_cache_breakpoint_count(root: &Map<String, Value>) -> usize {
    helpers::existing_cache_breakpoint_count(root)
}

/// Remove whitespace-only text blocks, empty content arrays, and empty
/// messages, migrating orphaned markers to prior cacheable blocks.
pub fn sanitize_claude_body(body: &mut Value) {
    sanitize::body(body);
}

#[cfg(test)]
#[path = "claude_cache_control/tests.rs"]
mod tests;
