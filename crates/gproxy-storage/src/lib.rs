pub mod entities;
pub mod db;
pub mod bus;
pub mod snapshot;
pub mod traffic;

pub use bus::{ConfigEvent, ControlEvent, StorageBus, StorageBusConfig};
pub use snapshot::StorageSnapshot;
pub use gproxy_provider_core::{DownstreamTrafficEvent, UpstreamTrafficEvent};
use std::sync::{OnceLock, RwLock};

pub use traffic::{
    AdminCredentialInput, AdminDisallowInput, AdminKeyInput, AdminProviderInput, AdminUserInput,
    TrafficStorage,
};

static GLOBAL_STORAGE: OnceLock<RwLock<Option<TrafficStorage>>> = OnceLock::new();

pub fn set_global_storage(storage: TrafficStorage) {
    let lock = GLOBAL_STORAGE.get_or_init(|| RwLock::new(None));
    let mut guard = lock.write().expect("global storage lock poisoned");
    *guard = Some(storage);
}

pub fn global_storage() -> Option<TrafficStorage> {
    let lock = GLOBAL_STORAGE.get_or_init(|| RwLock::new(None));
    let guard = lock.read().expect("global storage lock poisoned");
    guard.clone()
}
