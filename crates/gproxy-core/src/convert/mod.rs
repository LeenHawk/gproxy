//! Routing between passthrough and protocol conversion, plus the operation
//! spec lookups execution needs. Conversion flows arrive in later phases.

mod spec;

pub use spec::{is_websocket, response_framing, spec_for};
