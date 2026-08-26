//! The first embedder of `gproxy-core`: bootstrap, snapshots, and host services.

mod admin;
mod bootstrap;
mod cache;
mod cleanup;
mod config;
mod control;
mod error;
mod host;
pub mod ingress;
mod lifecycle;
mod logging;
mod secrets;

pub use config::Config;
pub use control::{ControlMutation, MutationResult};
pub use error::{AppError, ConfigError};
pub use lifecycle::{App, AppHandle};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Shared<T> = std::sync::Arc<T>;
#[cfg(target_arch = "wasm32")]
pub(crate) type Shared<T> = std::rc::Rc<T>;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
