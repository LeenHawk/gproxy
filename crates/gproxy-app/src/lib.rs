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
mod invalidation;
mod key_rotation;
mod lifecycle;
mod logging;
#[cfg(not(target_arch = "wasm32"))]
mod migrate_v2;
mod secrets;

pub use config::{Config, MasterKeyConfig};
#[cfg(not(target_arch = "wasm32"))]
pub use config::{LogFormat, NativeCommand};
pub use control::{ControlMutation, MutationResult};
pub use error::{AppError, ConfigError};
pub use lifecycle::{App, AppHandle};
#[cfg(not(target_arch = "wasm32"))]
pub use migrate_v2::{V2ImportOptions, V2ImportReport, migrate_from_v2};

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Shared<T> = std::sync::Arc<T>;
#[cfg(target_arch = "wasm32")]
pub(crate) type Shared<T> = std::rc::Rc<T>;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
