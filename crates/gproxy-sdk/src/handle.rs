//! The handle itself: one assembled engine plus everything core does not own.
//!
//! `Gproxy` is a cheap clone of a shared `Inner`. Cloning it shares the engine,
//! the store, the cache, the published routing table and the synchronization
//! loops; there is exactly one of each per assembled instance.

use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use gproxy_cache::Cache;
use gproxy_channel::ChannelDescriptor;
use gproxy_core::{ConfigRevision, Core, CredentialSummary, ReloadOutcome};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::Store;

use crate::{
    SdkError, SdkResult,
    builder::LoginTtl,
    resolve::{DEFAULT_MAX_ATTEMPTS, RotationCounters, RoutingTable},
    sync::{self, SyncHandle},
};

/// An assembled GPROXY instance.
pub struct Gproxy<C>(pub(crate) Arc<Inner<C>>);

// A derived Clone would demand `C: Clone`; the connection is never cloned,
// only shared.
impl<C> Clone for Gproxy<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Identity and position, never configuration: provider rows and credential
/// metadata are not safe to print.
impl<C> std::fmt::Debug for Gproxy<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gproxy")
            .field("instance", &self.0.instance)
            .field("revision", &self.0.core.snapshot().revision)
            .field("channels", &self.0.core.channels().len())
            .finish_non_exhaustive()
    }
}

pub(crate) struct Inner<C> {
    pub(crate) core: Core<C>,
    pub(crate) store: Arc<Store<C>>,
    pub(crate) cache: Arc<dyn Cache>,
    /// Routing rows of the active revision, republished by every reload.
    pub(crate) routing: ArcSwap<RoutingTable>,
    /// Balancing state, deliberately outside the routing table: a reload
    /// replaces the rows, not the position a round robin had reached.
    pub(crate) rotation: RotationCounters,
    /// Reloads are serialized, so a read and its publication cannot interleave
    /// with another reload's. A failed reload leaves the previous snapshot.
    pub(crate) reload_lock: tokio::sync::Mutex<()>,
    /// Set once, immediately after construction: the loops need a `Weak` to
    /// this very allocation, which does not exist until the `Arc` does.
    pub(crate) sync: OnceLock<SyncHandle>,
    pub(crate) login_ttl: LoginTtl,
    pub(crate) instance: Arc<str>,
}

// Dropping the last handle ends the background loops at their next wake even
// though they only hold a `Weak`: without this they would sleep out a whole
// poll interval first.
impl<C> Drop for Inner<C> {
    fn drop(&mut self) {
        if let Some(sync) = self.sync.get() {
            sync.cancel();
        }
    }
}

impl<C> Gproxy<C> {
    /// The engine. Execution, observation and settlement are its API, not this
    /// crate's; the handle adds what surrounds a call.
    pub fn core(&self) -> &Core<C> {
        &self.0.core
    }
    /// The durable rows. Reads are free; writes belong to the management
    /// families so the revision and the peers stay correct.
    pub fn store(&self) -> &Arc<Store<C>> {
        &self.0.store
    }
    /// Shared transient state, also the invalidation transport between peers.
    pub fn cache(&self) -> &Arc<dyn Cache> {
        &self.0.cache
    }
    /// This process, as channels and continuations see it.
    pub fn instance_id(&self) -> &Arc<str> {
        &self.0.instance
    }
    pub fn login_ttl(&self) -> LoginTtl {
        self.0.login_ttl
    }
    /// The configuration revision this instance is serving. It is not the
    /// durable one: a peer's write is only visible here after a reload.
    pub fn revision(&self) -> ConfigRevision {
        self.0.core.snapshot().revision
    }
    /// Routing rows of the active revision.
    pub fn routing(&self) -> Arc<RoutingTable> {
        self.0.routing.load_full()
    }
    /// Every compiled-in channel as data, ordered by id: what a management UI
    /// renders its provider forms from.
    pub fn channels(&self) -> Vec<ChannelDescriptor> {
        let registry = self.0.core.channels();
        let mut out: Vec<ChannelDescriptor> = registry
            .ids()
            .filter_map(|id| registry.get(id))
            .map(|channel| channel.descriptor())
            .collect();
        out.sort_by_key(|descriptor| descriptor.id);
        out
    }
    /// Stop the background synchronization loops. The handle stays usable;
    /// `reload` and `tick` still work, nothing reloads on its own any more.
    pub fn shutdown(&self) {
        if let Some(sync) = self.0.sync.get() {
            sync.cancel();
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Gproxy<C> {
    /// Re-read configuration and publish it: core's execution snapshot and the
    /// routing table, from the same durable state. Failure leaves both as they
    /// were; requests already in flight keep the view they pinned.
    pub async fn reload(&self) -> SdkResult<ReloadOutcome> {
        self.0.reload().await
    }

    /// Re-read persisted credential material after a peer rotated it. Cheaper
    /// than a reload and enough for a secret/status change; a credential row
    /// that did not exist at the last reload needs `reload` instead.
    pub async fn reload_credentials(
        &self,
        credential_ids: &[String],
    ) -> SdkResult<Vec<Option<CredentialSummary>>> {
        self.0.reload_credentials(credential_ids).await
    }

    /// One revision poll: reload when the durable `config_revision` is ahead
    /// of the active snapshot, otherwise do nothing. This is the backstop that
    /// makes a lost notification harmless.
    pub async fn sync_now(&self) -> SdkResult<Option<ReloadOutcome>> {
        sync::poll_once(&self.0).await
    }

    /// Apply the invalidations already waiting on the shared cache, without
    /// asking the database anything. Cheap enough to call per request where
    /// the revision poll is not: a host that wants both calls `tick`.
    /// Does nothing in `SyncMode::Background`, where the loop owns the
    /// subscription.
    pub async fn drain_notifications(&self) -> SdkResult<Option<ReloadOutcome>> {
        sync::drain(&self.0).await
    }

    /// One synchronization step for a host that has no background loop: apply
    /// whatever invalidations are already waiting, then poll the revision.
    /// This is what an edge isolate calls at the top of a request; natively
    /// with `SyncMode::Background` it is redundant but harmless.
    pub async fn tick(&self) -> SdkResult<Option<ReloadOutcome>> {
        let drained = sync::drain(&self.0).await?;
        Ok(sync::poll_once(&self.0).await?.or(drained))
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Inner<C> {
    pub(crate) async fn reload(self: &Arc<Self>) -> SdkResult<ReloadOutcome> {
        let _guard = self.reload_lock.lock().await;
        let outcome = self.core.reload_data().await?;
        let routing = self.store.load_routing_data().await?;
        // The instance-wide attempt budget lives on the settings row rather
        // than in `CoreData`, and resolution needs it for every name that does
        // not go through a route.
        let max_attempts = self
            .store
            .settings()
            .get()
            .await?
            .map_or(DEFAULT_MAX_ATTEMPTS, |settings| settings.max_attempts);
        self.routing.store(Arc::new(RoutingTable::new(
            outcome.active_revision,
            routing,
            max_attempts,
        )));
        Ok(outcome)
    }

    pub(crate) async fn reload_credentials(
        self: &Arc<Self>,
        credential_ids: &[String],
    ) -> SdkResult<Vec<Option<CredentialSummary>>> {
        Ok(self.core.reload_credentials(credential_ids).await?)
    }

    /// The durable revision, read straight from the settings row. A missing
    /// row is a broken installation rather than revision zero: the builder
    /// creates it and `commit_revision` needs it.
    pub(crate) async fn durable_revision(&self) -> SdkResult<i64> {
        self.store
            .settings()
            .get()
            .await?
            .map(|settings| settings.config_revision)
            .ok_or_else(|| SdkError::invalid("the global settings row is missing"))
    }
}
