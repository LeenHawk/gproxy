//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.

#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "claudecode")]
pub mod claudecode;
#[cfg(feature = "claudeweb")]
pub mod claudeweb;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "custom")]
pub mod custom;
#[cfg(any(
    feature = "azure",
    feature = "codex",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "custom",
    feature = "vertex",
    feature = "vertexexpress"
))]
mod shared;
#[cfg(feature = "vertex")]
pub mod vertex;
#[cfg(feature = "vertexexpress")]
pub mod vertexexpress;
