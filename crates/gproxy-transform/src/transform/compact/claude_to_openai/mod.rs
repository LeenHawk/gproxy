//! Claude -> OpenAI compact-content transforms.

mod input;
mod output;
mod request;
mod tools;
mod util;

const DEFAULT_MODEL: &str = "unknown";

pub(crate) use input::claude_messages_to_openai_items;
pub use output::response;
pub use request::request;
pub(crate) use tools::{
    apply_patch_result, prepare_response_output_item, server_tool_call, shell_result,
    tool_search_result, typed_tool_call,
};
