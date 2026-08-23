//! Canonical upstream transport for native and edge hosts.

mod buffered;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
mod wasm_socket;

#[cfg(not(target_arch = "wasm32"))]
pub use native::WreqTransport;
#[cfg(target_arch = "wasm32")]
pub use wasm::FetchTransport;

#[cfg(target_arch = "wasm32")]
pub type Transport = FetchTransport;
#[cfg(not(target_arch = "wasm32"))]
pub type Transport = WreqTransport;
