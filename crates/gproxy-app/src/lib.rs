//! The first embedder of `gproxy-core`: bootstrap, snapshots, and host services.

mod bootstrap;
mod cache;
mod config;
mod control;
mod error;
mod host;
mod lifecycle;
mod secrets;

pub use config::Config;
pub use control::{ControlMutation, MutationResult};
pub use error::{AppError, ConfigError};
pub use lifecycle::{App, AppHandle};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
