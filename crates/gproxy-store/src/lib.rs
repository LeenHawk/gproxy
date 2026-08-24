//! Shared persistence for native and edge hosts.

mod backend;
mod error;
mod migration;
mod query;
pub mod records;
pub mod schema;
mod store;

pub use backend::BackendConfig;
pub use error::StoreError;
pub use store::Store;
