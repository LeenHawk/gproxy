//! Shared persistence for native and edge hosts.

mod backend;
mod cache;
mod error;
mod migration;
mod query;
pub mod records;
pub mod schema;
mod store;

pub use backend::BackendConfig;
#[cfg(not(target_arch = "wasm32"))]
pub use cache::RedisCache;
pub use cache::{InProcessCache, LibsqlCache, UpstashCache};
pub use error::StoreError;
pub use store::{CleanupResult, Store};
