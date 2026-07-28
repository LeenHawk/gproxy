//! Outbound HTTP implementations for the host-owned channel transport.

#[cfg(not(target_arch = "wasm32"))]
pub use crate::channel::transport::ConduitSocket;
pub use crate::channel::transport::{
    ByteStreamDecoder, ClientError, RespStream, TransportOptions, UpstreamClient,
};

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod pool;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use pool::ClientPool;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod proxy_url;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod wreq;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use wreq::WreqClient;

#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
mod fetch;
#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
pub use fetch::FetchClient;
