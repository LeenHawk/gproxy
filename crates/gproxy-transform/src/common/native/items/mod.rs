mod calls;
mod claude;
mod execution;
mod hosted;
mod ids;

use gproxy_protocol::claude as claude_protocol;

pub(crate) use calls::{openai_call, request_block, response_block};
pub(crate) use claude::{claude_call, claude_result};
pub(crate) use ids::item_id;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeKind {
    Shell,
    ApplyPatch,
}

pub(crate) struct ClaudeCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input: claude_protocol::JsonObject,
}

pub(crate) fn is_buffered_native(name: &str) -> bool {
    matches!(
        name,
        "bash" | "str_replace_editor" | "str_replace_based_edit_tool"
    )
}
