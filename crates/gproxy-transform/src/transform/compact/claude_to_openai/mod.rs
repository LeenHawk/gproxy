//! Claude -> OpenAI compact-content transforms.

mod input;
mod output;
mod request;
mod tools;
mod util;

const DEFAULT_MODEL: &str = "unknown";
const DEFAULT_REASONING_ID: &str = "reasoning";

pub use output::response;
pub use request::request;
