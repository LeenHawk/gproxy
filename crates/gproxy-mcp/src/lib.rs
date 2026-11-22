//! MCP servers (Model Context Protocol, streamable HTTP) as upstreams.
//!
//! A provider of kind [`Mcp`] is one server. [`client`] talks to it, [`server`]
//! reads what a caller sent and shapes the answers, [`scope`] decides which
//! tools a caller may use, [`tool`] names them and reads their nature,
//! [`binding`] remembers which upstream session a caller's session uses and
//! [`audit`] is the record of one tool call. The host joins them: it owns the
//! credentials, the storage and the HTTP route.
//!
//! Messages are handled as JSON values, not as `rmcp`'s typed envelopes: a
//! gateway has to hand on fields it does not know. `rmcp` supplies the types
//! of what the gateway does read.

pub mod audit;
pub mod binding;
pub mod client;
mod kind;
pub mod scope;
pub mod server;
pub mod tool;

pub use kind::{ID, Mcp, request_headers};
pub use rmcp::model::{ProtocolVersion, Tool};
