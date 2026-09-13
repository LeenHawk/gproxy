//! Strict buffered Claude Messages stream collection and event synthesis.

mod blocks;
mod collector;
mod synthesize;
mod usage;

pub use collector::{ClaudeStreamCollector, ClaudeStreamLimits};
pub use synthesize::synthesize_claude_stream;
