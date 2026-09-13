//! GPROXY v4 protocol.
//!
//! Two jobs, and nothing else:
//!
//! 1. **Model the connection.** What a request and a response are on an
//!    HTTP link — five request elements, three response elements, and buffered
//!    or streaming bodies. An established WebSocket is a separate duplex link.
//! 2. **Convert between vendor dialects.** Pairwise, in both directions.
//!
//! What is deliberately absent:
//!
//! - **URL path matching.** Which URL serves an operation is an HTTP
//!   convention. The ingress layer owns it; an SDK caller names the operation
//!   and never sees a path.
//! - **Any notion of a channel.** A channel is a vendor integration with
//!   credentials and auth; this crate only knows dialects.
//! - **A unified IR.** Conversion is N-by-N pairwise on purpose. Upstream specs
//!   churn faster than any pivot format tracks, and conversion fidelity is the
//!   product. See `design/transform.md`.
//!
//! Everything here derives from `upstream_docs/`, which is the source of truth
//! for field names, semantics and examples.

pub mod claude;
pub mod connection;
pub mod gemini;
pub mod openai;
pub mod operation;
pub mod spec;

pub use connection::{HttpBody, WebSocket, WireRequest, WireResponse};
pub use operation::{Dialect, Operation, OperationKey, WireFamily};

/// Unknown fields preserved when a wire object is read and written unchanged.
pub type Rest = serde_json::Map<String, serde_json::Value>;
