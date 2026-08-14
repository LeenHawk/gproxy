//! OpenAI -> Claude compact-content transforms.

mod input;
mod output;
mod request;
mod tools;
mod util;

const DEFAULT_COMPACT_MAX_TOKENS: u64 = 32_768;
const DEFAULT_MODEL: &str = "unknown";

pub(crate) use input::openai_input_to_claude_messages;
pub use output::response;
pub use request::{request, request_headers};
pub(crate) use tools::{
    apply_patch_to_text_editor_input, local_shell_to_bash_input, shell_to_bash_input,
    web_action_to_claude,
};
