//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.

#[cfg(feature = "claudecode")]
pub mod claudecode;
#[cfg(feature = "claudeweb")]
pub mod claudeweb;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "custom")]
pub mod custom;
#[cfg(any(feature = "codex", feature = "claudecode"))]
mod services_common;
