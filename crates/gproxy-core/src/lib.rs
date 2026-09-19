//! Engine data contracts. No HTTP routing, request execution, configuration
//! compilation or background workers are started by these types.
//! Store owns durable facts, Cache owns shared transient state, and CoreData
//! holds each instance's immutable configuration snapshot.

#![forbid(unsafe_code)]

pub mod api;
pub mod assemble;
#[cfg(not(target_arch = "wasm32"))]
mod availability;
pub mod builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod capability;
pub mod context;
pub mod convert;
pub mod data;
#[cfg(not(target_arch = "wasm32"))]
mod execute;
#[cfg(not(target_arch = "wasm32"))]
mod ids;
pub mod keys;
pub mod limits;
pub mod observe;
#[cfg(not(target_arch = "wasm32"))]
mod quota;
#[cfg(not(target_arch = "wasm32"))]
mod refresh;
pub mod rewrite;
pub mod runtime;
pub mod secret;
#[cfg(not(target_arch = "wasm32"))]
mod select;
#[cfg(not(target_arch = "wasm32"))]
mod session;

pub use api::*;
pub use assemble::AssemblyError;
pub use builder::*;
#[cfg(not(target_arch = "wasm32"))]
pub use capability::*;
pub use context::*;
pub use data::*;
pub use limits::*;
pub use observe::*;
pub use rewrite::RewriteCompileError;
pub use runtime::*;
pub use secret::*;

use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_channel::ChannelRegistry;
use gproxy_store::Store;
use std::sync::Arc;

/// Assembled engine dependencies, built through `CoreBuilder`. Construction
/// performs no I/O. The host supplies Store, shared Cache, Observer, secret
/// codec and channel registry; core owns the outbound client pool. There is no
/// implicit backend and no default observer that silently drops settlement.
pub struct Core<C> {
    store: Arc<Store<C>>,
    cache: Arc<dyn Cache>,
    observer: Arc<dyn Observer>,
    codec: Arc<dyn SecretCodec>,
    channels: Arc<ChannelRegistry>,
    #[cfg(not(target_arch = "wasm32"))]
    clients: gproxy_client::ClientPool,
    #[cfg(not(target_arch = "wasm32"))]
    files: Option<gproxy_file::Operator>,
    data: ArcSwap<CoreData>,
}
impl<C> Core<C> {
    pub fn builder(store: Arc<Store<C>>) -> CoreBuilder<C> {
        CoreBuilder::new(store)
    }
    /// Pin one immutable configuration snapshot for the logical request.
    pub fn snapshot(&self) -> Arc<CoreData> {
        self.data.load_full()
    }
    /// The Store this engine reads and writes. Management writes go through the
    /// upper layer's coordinator; this accessor exists for tests and diagnostics.
    pub fn store(&self) -> &Arc<Store<C>> {
        &self.store
    }
    pub fn cache(&self) -> &Arc<dyn Cache> {
        &self.cache
    }
    pub fn observer(&self) -> &Arc<dyn Observer> {
        &self.observer
    }
    /// Object storage for locally published bodies, when configured.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn file_storage(&self) -> Option<&gproxy_file::Operator> {
        self.files.as_ref()
    }
    pub fn secret_codec(&self) -> &Arc<dyn SecretCodec> {
        &self.codec
    }
    pub fn channels(&self) -> &Arc<ChannelRegistry> {
        &self.channels
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn clients(&self) -> &gproxy_client::ClientPool {
        &self.clients
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
    pub fn into_parts(self) -> CoreParts<C> {
        CoreParts {
            store: self.store,
            cache: self.cache,
            observer: self.observer,
            codec: self.codec,
            channels: self.channels,
            data: self.data.into_inner(),
        }
    }
}

/// Ownership recovered by `Core::into_parts`. The client pool is dropped with
/// the engine; live sockets already handed out are unaffected.
pub struct CoreParts<C> {
    pub store: Arc<Store<C>>,
    pub cache: Arc<dyn Cache>,
    pub observer: Arc<dyn Observer>,
    pub codec: Arc<dyn SecretCodec>,
    pub channels: Arc<ChannelRegistry>,
    pub data: Arc<CoreData>,
}
