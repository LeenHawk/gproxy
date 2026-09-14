//! Direct incremental Claude Messages ↔ Gemini GenerateContent streams.
//!
//! Source and target native collectors independently validate both lifecycles.
//! Live text comes from actual fragments. Complete-source conversion is used
//! only after termination for final usage/metadata and full canonical checks;
//! emitted text is never replayed. Adjacent plain Gemini text fragments with
//! the same thought flag are normalized only for canonical comparison.
//!
//! Claude blocks retain their native order, including interleaved tools and
//! thoughts. The first text block remains live; later blocks wait until earlier
//! blocks close. Initial text/tool input is preserved, and tool JSON completes
//! before an immutable Gemini function call is emitted.
//!
//! Claude target start needs actual model and usage facts. Without them, target
//! fragments wait in a bounded queue until a coherent native usage observation
//! arrives. Missing source IDs can allocate stable aliases; late IDs attach to
//! them. Context input/cache facts are fixed for this invocation. Final thinking
//! facts do not become initial counters, and older cumulative components are
//! not mistaken for final values when the native total grows.
//!
//! Every failed push poisons the converter. All output events, including start
//! and terminal events, are charged once before native validation/retention.
//! Required opaque replay, media/execution output and continuation state need
//! an invocation adapter. Omitted optional metadata is reported separately.
//! These contracts do not establish empirical equality of model judgments.
mod claude_to_gemini;
mod common;
mod gemini_to_claude;
pub use claude_to_gemini::{ClaudeToGeminiContext, ClaudeToGeminiStream};
pub use common::{StreamEnd, StreamLimits};
pub use gemini_to_claude::{GeminiToClaudeContext, GeminiToClaudeStream};

mod claude_blocks;
mod usage;
