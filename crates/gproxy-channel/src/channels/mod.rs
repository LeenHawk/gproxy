//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.

#[cfg(feature = "claudecode")]
pub mod claudecode;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "custom")]
pub mod custom;
