//! Edge inbound entry: bridges a WinterCG `fetch` event into the request
//! pipeline.
//!
//! [`init`] builds the shared [`AppState`](crate::app::AppState) from
//! host-supplied credentials. [`fetch`] dispatches requests directly to the
//! shared pipeline because the wasm transports do not produce `Send` futures
//! required by the native axum router.

mod bridge;
mod dispatch;
pub(crate) mod http;
mod init;

pub use dispatch::{fetch, responses_websocket_frame};
pub use init::init;
