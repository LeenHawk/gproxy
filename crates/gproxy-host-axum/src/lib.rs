//! Native axum host for `gproxy-app`.

#[cfg(not(target_arch = "wasm32"))]
mod ingress;
#[cfg(not(target_arch = "wasm32"))]
mod response;
#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
mod websocket;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{AxumServer, HostError};
