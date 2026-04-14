//! gproxy SDK — layered facade over `gproxy-protocol`, `gproxy-channel`, and
//! `gproxy-engine`.
//!
//! Pick the layer that matches your need:
//!
//! - [`protocol`] — wire-format types for Claude / OpenAI / Gemini plus
//!   cross-protocol `TryFrom` transforms and a runtime transform dispatcher.
//!   Light dependencies, no HTTP.
//! - [`channel`] — the `Channel` trait, concrete channel implementations
//!   (OpenAI, Anthropic, Gemini and friends), credential types, request /
//!   response types, health tracking, and token counting. Add `gproxy-channel`
//!   when you want a strongly typed single-provider client.
//! - [`engine`] — the full multi-channel `GproxyEngine`, provider store,
//!   retry / credential affinity, backend traits for rate-limit / quota /
//!   affinity state, and routing helpers. Add `gproxy-engine` when you want
//!   to build your own LLM gateway.

pub use gproxy_channel as channel;
pub use gproxy_engine as engine;
pub use gproxy_protocol as protocol;
