//! OpenAI -> Claude compact-content transforms.

mod input;
mod output;
mod request;
mod tools;
mod util;

const DEFAULT_COMPACT_MAX_TOKENS: u64 = 32_768;
const DEFAULT_MODEL: &str = "unknown";

pub use output::response;
pub use request::{request, request_headers};
