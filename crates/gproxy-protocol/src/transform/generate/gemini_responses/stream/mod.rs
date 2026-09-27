//! Direct, bounded Gemini GenerateContent ↔ Responses incremental conversion.
//! Native source/target collectors validate lifecycle and final canonical parity.
//!
//! Constructors accept an explicit target policy before allocating any IDs.
//! Gemini response IDs may arrive late: the already emitted Responses ID stays
//! fixed while its actual source association is attached. A Responses header
//! requires an observed or explicit actual model and factual creation time;
//! the client request's routing alias is never used as a model observation.
//!
//! Gemini parts become distinct Responses items, including repeated text and
//! same-name functions. Text, thinking and actual complete function arguments
//! are emitted immediately. Item status and required usage are finalized only
//! after native termination and EOF. Gemini usage must be observed at or after
//! the last content; earlier thinking/candidate components remain lower bounds
//! until current totals/components or an explicit final thinking fact resolve
//! the final split. Missing counters are not fabricated as zero.
//!
//! Responses message parts retain source order because Gemini has no indexed
//! text deltas. Later parts/items wait boundedly for earlier content to close;
//! a complete function argument object may be emitted before output_item.done.
//! Reasoning content takes precedence over summaries, so summaries wait until
//! item completion. Final canonical comparison coalesces only adjacent plain
//! text fragments with the same thought flag; signatures/media/functions stay
//! exact. Unsigned empty reasoning is omitted like the buffered pair.
//!
//! A Responses upstream's reasoning ciphertext cannot become a Gemini
//! signature, so reasoning replays as unsigned thought text. This pure
//! converter itself performs no state writes or upstream calls.
//!
//! Every failed push poisons the stream. Input/output events and bytes, pending
//! payloads, items, parts and tools are bounded. Provider errors, premature EOF,
//! unsupported execution/media and conflicting factual metadata cannot produce
//! a successful target terminal. Annotations/logprobs/usage retain actual final
//! native facts rather than replaying previously emitted text.

mod common;
mod context;
mod gemini_parts;
mod gemini_to_responses;
mod identity;
mod response_events;
mod response_items;
mod response_projection;
mod responses_to_gemini;
mod usage;
pub use common::{StreamEnd, StreamLimits};
pub use context::{GeminiToResponsesContext, ResponsesToGeminiContext};
pub use gemini_to_responses::GeminiToResponsesStream;
pub use responses_to_gemini::ResponsesToGeminiStream;
