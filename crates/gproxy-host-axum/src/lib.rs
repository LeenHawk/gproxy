//! Native axum host for `gproxy-app`.

#[cfg(not(target_arch = "wasm32"))]
mod ingress;
#[cfg(not(target_arch = "wasm32"))]
mod response;
#[cfg(not(target_arch = "wasm32"))]
mod server;
#[cfg(not(target_arch = "wasm32"))]
mod static_assets;
#[cfg(not(target_arch = "wasm32"))]
mod websocket;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{AxumServer, HostError};

pub const UPDATE_SIGNING_PUBLIC_KEY: Option<&str> = option_env!("GPROXY_UPDATE_PUBKEY");
