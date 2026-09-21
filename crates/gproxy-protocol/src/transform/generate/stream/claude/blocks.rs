use super::collector::invalid;
use crate::{
    transform::TransformError,
    wire::claude::{generate_content::ResponseContentBlock as B, stream::ContentBlockDelta as D},
};

type Input = serde_json::Map<String, serde_json::Value>;

pub(super) fn input(block: &mut B) -> Option<&mut Input> {
    match block {
        B::ToolUse(b) => Some(&mut b.input),
        B::ServerToolUse(b) => Some(&mut b.input),
        B::McpToolUse(b) => Some(&mut b.input),
        _ => None,
    }
}

pub(super) fn call_id(block: &B) -> Option<&str> {
    match block {
        B::ToolUse(b) => Some(&b.id),
        B::ServerToolUse(b) => Some(&b.id),
        B::McpToolUse(b) => Some(&b.id),
        _ => None,
    }
}

pub(super) fn text_bytes(block: &B) -> usize {
    match block {
        B::Text(b) => b.text.len(),
        B::Thinking(b) => b.thinking.len(),
        B::Compaction(b) => b
            .content
            .as_ref()
            .and_then(|x| x.as_ref())
            .map_or(0, String::len),
        _ => 0,
    }
}

pub(super) fn delta_text_bytes(delta: &D) -> usize {
    match delta {
        D::Text(b) => b.text.len(),
        D::Thinking(b) => b.thinking.len(),
        D::Compaction(b) => b
            .content
            .as_ref()
            .and_then(|x| x.as_ref())
            .map_or(0, String::len),
        _ => 0,
    }
}

pub(super) fn apply(block: &mut B, delta: D, buffer: &mut String) -> Result<(), TransformError> {
    match (delta, block) {
        (D::Text(d), B::Text(b)) => b.text.push_str(&d.text),
        (D::Thinking(d), B::Thinking(b)) => b.thinking.push_str(&d.thinking),
        // The native SDK treats signatures and compaction payloads as snapshots.
        (D::Signature(d), B::Thinking(b)) => b.signature = d.signature,
        (D::Compaction(d), B::Compaction(b)) => {
            if d.content.is_some() {
                b.content = d.content;
            }
            if d.encrypted_content.is_some() {
                b.encrypted_content = d.encrypted_content;
            }
        }
        (D::Citations(d), B::Text(b)) => b
            .citations
            .get_or_insert_with(|| Some(Vec::new()))
            .get_or_insert_with(Vec::new)
            .push(d.citation),
        (D::InputJson(d), b) => {
            if input(b).is_none() {
                return Err(invalid(
                    "input_json_delta",
                    "delta does not match tool block",
                ));
            }
            // Initial nonempty input plus a JSON stream is ambiguous: never
            // silently replace substantive start content with unrelated input.
            if buffer.is_empty() && input(b).is_some_and(|i| !i.is_empty()) {
                return Err(invalid(
                    "tool.input",
                    "nonempty start input followed by JSON deltas",
                ));
            }
            buffer.push_str(&d.partial_json);
        }
        _ => {
            return Err(invalid(
                "content_block_delta",
                "delta does not match content block",
            ));
        }
    }
    Ok(())
}
