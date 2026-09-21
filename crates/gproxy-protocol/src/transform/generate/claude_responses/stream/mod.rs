//! Direct incremental Claude Messages ↔ OpenAI Responses streams.
//!
//! Both native source and target collectors validate the lifecycle. Live text,
//! reasoning and function arguments come from actual source fragments. Final
//! source conversion checks the full target and supplies terminal metadata;
//! it never replays already emitted text. JSON argument spelling may differ,
//! but the streamed object must equal the actual native source tool input.
//!
//! Claude-to-Responses uses factual creation time and original/effective request
//! controls. Each Claude block retains its own output item, and call IDs remain
//! distinct from function item IDs. A late global Claude refusal keeps emitted
//! output text and becomes incomplete/content_filter. It cannot retract that
//! text into another channel. Annotation coordinates/resource bindings require
//! explicit facts bound to the exact source citation occurrence.
//!
//! Responses-to-Claude preserves every text/refusal part and actual tool input.
//! Earlier messages gate later items until their part list is complete; known
//! function items can stream concurrently. Optional item IDs may arrive late,
//! while native call IDs determine the target tool identity. Per-part refusal
//! text is retained; Claude's single stop_reason preserves explicit tool
//! continuation when both coexist, with a separate representation diagnostic.
//!
//! A Claude target start requires real usage. Missing start facts wait in a
//! bounded queue; initial counters never replace missing final usage. Initial
//! host input/cache facts must agree with final native counters. Late effective
//! service tier cannot rewrite message_start and is reported separately.
//! Signed thinking restores only from validated original model/origin/field
//! state. MCP receipts retain native MCP shapes without executing tools.
//!
//! Every failed push poisons the converter. Source terminal events and finish
//! are mandatory. Input/output bytes/events and retained payloads are bounded,
//! and each target event is charged once before native validation/retention.

mod claude_blocks;
mod claude_to_responses;
mod common;
mod context;
mod response_events;
mod response_items;
mod response_projection;
mod responses_to_claude;
mod usage;

pub use claude_to_responses::{ClaudeToResponsesContext, ClaudeToResponsesStream};
pub use common::{StreamEnd, StreamLimits};
pub use context::BoundResponseAnnotation;
pub use responses_to_claude::{ResponsesToClaudeContext, ResponsesToClaudeStream};
