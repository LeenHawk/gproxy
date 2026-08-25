//! Fetch-runtime host for `gproxy-app`.

#[cfg(target_arch = "wasm32")]
mod edge;
#[cfg(target_arch = "wasm32")]
mod request;
#[cfg(target_arch = "wasm32")]
mod response;
#[cfg(target_arch = "wasm32")]
mod stream;
#[cfg(target_arch = "wasm32")]
mod websocket;

#[cfg(target_arch = "wasm32")]
pub use edge::{EdgeHost, EdgeReply, start};
