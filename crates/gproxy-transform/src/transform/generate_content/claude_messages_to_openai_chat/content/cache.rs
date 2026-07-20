use crate::protocol::{claude, openai};

use super::super::super::common::openai_breakpoint;

pub(super) fn breakpoint_for_text(
    text: &str,
    cache_control: Option<claude::CacheControl>,
    target: &str,
) -> Option<openai::PromptCacheBreakpoint> {
    if text.trim().is_empty() {
        if cache_control.is_some() {
            warn_dropped_cache_breakpoint("text", target);
        }
        None
    } else {
        openai_breakpoint(cache_control)
    }
}

pub(super) fn warn_dropped_cache_breakpoint(block_type: &str, target: &str) {
    tracing::warn!(
        block_type,
        conversion_target = target,
        "cache breakpoint dropped during protocol conversion"
    );
}

pub(super) fn warn_unrepresentable_cache_control(block: &claude::ContentBlockParam, target: &str) {
    let Ok(value) = serde_json::to_value(block) else {
        return;
    };
    if value.get("cache_control").is_some() {
        warn_dropped_cache_breakpoint(
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            target,
        );
    }
}
