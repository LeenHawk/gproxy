//! GPROXY v3 channel contract.
//!
//! A channel owns provider-specific knowledge: upstream URLs, auth
//! injection, request shaping, response classification, stream decoding,
//! usage extraction, OAuth refresh, and its service-surface table. The
//! engine owns everything else — routing, admission, failover, settlement.
//!
//! The [`Channel`](channel::Channel) trait is deliberately synchronous and
//! object-safe: adapters are pure logic over borrowed data, so a registry
//! can hold `Box<dyn Channel>` with no async-trait machinery. The one
//! async concern — credential refresh — returns a boxed future from a
//! plain method.

pub mod channel;
pub mod disposition;
pub mod registry;
pub mod surface;
pub mod usage;

pub use channel::{
    Channel, ChannelDescriptor, ChannelError, Frame, PrepareCtx, PreparedRequest, ResponseView,
    SimpleHttp, StreamDecoder, StreamTail,
};
pub use disposition::Disposition;
pub use registry::ChannelRegistry;
pub use surface::{
    ForwardSpec, SurfaceAction, SurfaceEntry, SurfaceTable, SynthCtx, SynthReply, Synthesizer,
};
pub use usage::NormalizedUsage;

/// Boxed future with the wasm `Send` split — the one language-level tax
/// this crate carries for the single-threaded wasm target.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + 'a>>;
