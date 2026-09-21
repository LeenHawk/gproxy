//! Bounded native Gemini stream collection and buffered response synthesis.

mod collector;
mod merge;
mod synthesize;
pub use collector::{GeminiStreamCollector, GeminiStreamLimits};
pub use synthesize::synthesize_gemini_stream;
