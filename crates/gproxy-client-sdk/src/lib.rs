//! Programs calling GPROXY use this crate. The embeddable engine is
//! `gproxy-sdk`; the outbound HTTP transport is `gproxy-client`.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
pub mod assets;
pub mod backend;
mod client;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub mod dialects;
#[cfg(feature = "embeddings")]
pub mod embeddings;
mod error;
pub mod ir;
pub mod operations;
pub mod options;
pub mod stream;
#[cfg(feature = "ts")]
mod typescript;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub use assets::PreparedGenerateRequest;
#[cfg(feature = "s3")]
pub use assets::s3;
pub use assets::{
    AssetDialect, DeliveryConstraints, DeliveryMethod, DeliveryMode, MediaCapability, MediaKind,
    media_capability,
};
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "s3"
))]
pub use assets::{AssetLocation, ResolveAsset, ResolvedAsset};
pub use backend::Backend;
#[cfg(any(feature = "embeddings", feature = "video"))]
pub(crate) use client::path_segment;
#[cfg(any(
    feature = "embeddings",
    feature = "rerank",
    feature = "audio",
    feature = "images",
    feature = "video",
    all(
        feature = "count-tokens",
        any(feature = "claude", feature = "openai", feature = "gemini")
    )
))]
pub(crate) use client::wire_json;
pub use client::{Client, DEFAULT_LIMITS};
pub use error::{Error, ErrorKind, Result};
pub use gproxy_protocol as protocol;
pub(crate) use gproxy_protocol::{HttpBody, OperationKey, WireRequest, WireResponse};
pub use ir::*;
pub use stream::{Event, GenerationStream, aggregate};
#[cfg(feature = "ts")]
pub use typescript::export_types;
