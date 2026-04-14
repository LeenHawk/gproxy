//! gproxy SDK — layered facade over `gproxy-protocol`, `gproxy-channel`, and
//! `gproxy-engine`.
//!
//! Most users want `engine` (the multi-channel engine) or `channel` (a single
//! upstream with credentials + typed requests). `protocol` is exposed for
//! users who only need wire-format types and cross-protocol transforms.
//!
//! The older `provider` / `routing` aliases are preserved for backward
//! compatibility with code written before the SDK layer split. New code
//! should prefer `channel` / `engine`.

pub use gproxy_protocol as protocol;
pub use gproxy_channel as channel;
pub use gproxy_engine as engine;

// Backward-compat aliases — deprecated, will be removed in a future major.
pub use gproxy_provider as provider;
pub use gproxy_routing as routing;
