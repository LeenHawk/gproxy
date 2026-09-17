//! Engine data contracts. No HTTP routing, request execution, configuration
//! compilation or background workers are started by these types.
//! Store owns durable facts, Cache owns shared transient state, and CoreData
//! holds each instance's immutable configuration snapshot.

#![forbid(unsafe_code)]

pub mod api;
pub mod context;
pub mod data;
pub mod runtime;

pub use api::*;
pub use context::*;
pub use data::*;
pub use runtime::*;

use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_store::Store;
use std::sync::Arc;

/// Assembled engine dependencies. Construction performs no I/O. SDK/app supplies
/// the Store, shared Cache and prepared snapshot; there is no implicit backend.
pub struct Core<C> {
    store: Arc<Store<C>>,
    cache: Arc<dyn Cache>,
    data: ArcSwap<CoreData>,
}
impl<C> Core<C> {
    pub fn new(store: Arc<Store<C>>, cache: Arc<dyn Cache>, data: Arc<CoreData>) -> Self {
        Self {
            store,
            cache,
            data: ArcSwap::from(data),
        }
    }
    /// Pin one immutable configuration snapshot for the logical request.
    pub fn snapshot(&self) -> Arc<CoreData> {
        self.data.load_full()
    }
    pub fn cache(&self) -> &Arc<dyn Cache> {
        &self.cache
    }
    /// Publish an already-validated snapshot with a durable configuration
    /// revision. Delayed reloads cannot overwrite a newer revision. This does
    /// not persist revisions, compile data or authorize configuration changes.
    pub fn publish_snapshot(&self, next: Arc<CoreData>) -> bool {
        let previous = self.data.rcu(|current| {
            if next.revision > current.revision {
                next.clone()
            } else {
                current.clone()
            }
        });
        next.revision > previous.revision
    }
    /// Recover ownership when dismantling an engine without exposing a live
    /// writable Store accessor that bypasses future management coordination.
    pub fn into_parts(self) -> (Arc<Store<C>>, Arc<dyn Cache>, Arc<CoreData>) {
        (self.store, self.cache, self.data.into_inner())
    }
}
