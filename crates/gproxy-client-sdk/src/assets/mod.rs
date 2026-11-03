//! Delivery capabilities and request-scoped asset preparation.
pub mod capabilities;
pub use capabilities::{
    AssetDialect, DeliveryConstraints, DeliveryMethod, DeliveryMode, MediaCapability, MediaKind,
    media_capability,
};
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "s3",
    feature = "images",
    feature = "transcription"
))]
pub mod resolve;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub use resolve::PreparedGenerateRequest;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "s3",
    feature = "images",
    feature = "transcription"
))]
pub use resolve::{AssetLocation, ResolveAsset, ResolvedAsset};
#[cfg(feature = "s3")]
pub mod s3;

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "images",
    feature = "transcription"
))]
mod prepare;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings",
    feature = "images",
    feature = "transcription"
))]
mod validation;
