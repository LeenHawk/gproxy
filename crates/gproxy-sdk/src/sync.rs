//! Keeping every instance on the same configuration.
//!
//! Two independent mechanisms, because neither alone is enough. The shared
//! cache carries `Invalidation` notifications, which are fast but lossy: a
//! publish can fail, a subscriber can lag, a backend can drop the topic. The
//! durable `settings.config_revision` is the authority, polled on an interval,
//! which is slow but cannot miss anything. A notification therefore never
//! carries state — it only says "look again", and looking again is a reload.
//!
//! Reloads are monotonic and serialized: an older revision never replaces a
//! newer one, and a failed reload leaves the previous snapshot serving.

use std::{sync::Arc, time::Duration};

use gproxy_cache::{Cache, Notification, Subscription};
use gproxy_core::{Invalidation, ReloadOutcome};
use gproxy_seaorm::BatchConnectionTrait;
use tokio_util::sync::CancellationToken;

use crate::{SdkResult, handle::Inner, rt};

/// The topic `Invalidation` payloads travel on. Every instance of a deployment
/// publishes and subscribes here; it is core's constant, re-exported so a host
/// that publishes its own invalidations names the same string.
pub use gproxy_core::keys::INVALIDATION_TOPIC;

/// The default revision poll interval.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Resubscription backoff after the notification topic closes or refuses.
/// Only the background loop resubscribes; `tick` leaves a closed topic to the
/// revision poll of the same call.
#[cfg(not(target_arch = "wasm32"))]
const MIN_BACKOFF: Duration = Duration::from_secs(1);
#[cfg(not(target_arch = "wasm32"))]
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// A zero timeout: take what is already delivered, wait for nothing.
const NOTHING_PENDING: Duration = Duration::ZERO;

/// Who drives synchronization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyncMode {
    /// The handle runs its own subscription and poll loops. Needs a Tokio
    /// runtime and a process that outlives the request, so this is the native
    /// server mode; on wasm32 it behaves as `Manual`.
    #[default]
    Background,
    /// Nothing happens unless the host calls `tick` or `sync_now`. This is
    /// what an edge isolate wants, and what a test wants.
    Manual,
}

pub(crate) struct SyncHandle {
    cancel: CancellationToken,
    /// Only `Manual` keeps one: in `Background` the loop owns its own, and a
    /// second subscriber would race it for the same notifications.
    subscription: Option<tokio::sync::Mutex<Box<dyn Subscription>>>,
}

impl SyncHandle {
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// Everything that must happen before the initial reload: in `Manual` mode,
/// subscribe, so a write landing between now and the first `tick` is not
/// missed by both mechanisms.
pub(crate) async fn prepare(cache: &Arc<dyn Cache>, mode: SyncMode) -> SyncHandle {
    let manual = mode == SyncMode::Manual || cfg!(target_arch = "wasm32");
    SyncHandle {
        cancel: CancellationToken::new(),
        subscription: if manual { subscribe(cache).await } else { None },
    }
}

/// Subscribe and discard whatever is already queued. A fresh subscription
/// always opens with `ResyncRequired`; the caller's initial reload *is* that
/// resync, so delivering it to the first `tick` would only reload twice.
async fn subscribe(cache: &Arc<dyn Cache>) -> Option<tokio::sync::Mutex<Box<dyn Subscription>>> {
    match cache.subscribe(INVALIDATION_TOPIC).await {
        Ok(mut subscription) => {
            while let Some(Ok(_)) = rt::timeout(NOTHING_PENDING, subscription.recv()).await {}
            Some(tokio::sync::Mutex::new(subscription))
        }
        Err(error) => {
            // A cache without a usable topic is not fatal: the revision poll
            // still converges, just at its own interval.
            tracing::warn!(%error, "invalidation topic unavailable; relying on the revision poll");
            None
        }
    }
}

/// Start the background loops, if this target and mode have any.
pub(crate) fn spawn<C>(inner: &Arc<Inner<C>>, mode: SyncMode, poll_interval: Duration)
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        // An isolate does not outlive the request that created it; a loop here
        // would be cancelled mid-sleep and never poll. `tick` is the answer.
        let _ = (inner, mode, poll_interval);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if mode != SyncMode::Background {
            return;
        }
        let Some(sync) = inner.sync.get() else {
            return;
        };
        let cancel = sync.cancel.clone();
        let polling = rt::spawn(poll_loop(
            Arc::downgrade(inner),
            poll_interval,
            cancel.clone(),
        ));
        let listening = rt::spawn(subscription_loop(
            Arc::downgrade(inner),
            inner.cache.clone(),
            cancel,
        ));
        if !polling || !listening {
            tracing::warn!(
                "no Tokio runtime for background synchronization; build with SyncMode::Manual and call tick()"
            );
        }
    }
}

/// One revision poll. The durable number is the authority; the active snapshot
/// only ever moves forward towards it.
pub(crate) async fn poll_once<C>(inner: &Arc<Inner<C>>) -> SdkResult<Option<ReloadOutcome>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let durable = inner.durable_revision().await?;
    let active = inner.core.snapshot().revision;
    if u64::try_from(durable).unwrap_or(0) > active.0 {
        return inner.reload().await.map(Some);
    }
    Ok(None)
}

/// Apply every notification already waiting, without blocking on the next one.
/// Does nothing when this handle keeps no subscription of its own.
pub(crate) async fn drain<C>(inner: &Arc<Inner<C>>) -> SdkResult<Option<ReloadOutcome>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let Some(subscription) = inner.sync.get().and_then(|sync| sync.subscription.as_ref()) else {
        return Ok(None);
    };
    let mut subscription = subscription.lock().await;
    let mut last = None;
    // Stops on the first timeout (nothing pending) or on a closed topic;
    // neither is worth an error, because the revision poll of this same tick
    // is the authority and resubscription is the background loop's job.
    while let Some(Ok(notification)) = rt::timeout(NOTHING_PENDING, subscription.recv()).await {
        last = apply(inner, notification).await?.or(last);
    }
    Ok(last)
}

/// What one notification means for this instance. Anything unreadable, or
/// about something this snapshot has never seen, is a reason to reload rather
/// than to ignore: notifications are hints, and a wasted reload is cheap
/// compared with serving stale configuration.
async fn apply<C>(
    inner: &Arc<Inner<C>>,
    notification: Notification,
) -> SdkResult<Option<ReloadOutcome>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let payload = match notification {
        Notification::ResyncRequired => return inner.reload().await.map(Some),
        Notification::Message(payload) => payload,
    };
    let Ok(invalidation) = serde_json::from_slice::<Invalidation>(&payload) else {
        tracing::warn!("unreadable invalidation payload; reloading");
        return inner.reload().await.map(Some);
    };
    match invalidation {
        // A peer's write, or our own coming back to us. Older or equal means
        // this instance already has it.
        Invalidation::ConfigurationChanged { revision, .. } => {
            if revision > inner.core.snapshot().revision {
                return inner.reload().await.map(Some);
            }
            Ok(None)
        }
        Invalidation::CredentialChanged {
            credential_id,
            version,
        } => match inner.core.snapshot().credentials.get(&credential_id) {
            Some(slot) if slot.state.load().version >= version => Ok(None),
            // Material changed under a credential we already serve: re-read
            // that row, no snapshot rebuild.
            Some(_) => {
                inner.reload_credentials(&[credential_id]).await?;
                Ok(None)
            }
            // Created since our last reload, so there is no slot to update.
            None => inner.reload().await.map(Some),
        },
        // Removal is authoritative loading's decision, not the event's.
        Invalidation::CredentialRemoved { .. } => inner.reload().await.map(Some),
    }
}

/// The durable backstop: poll the revision, reload when it moved. Holds a
/// `Weak`, so dropping the last handle ends the loop at its next wake.
#[cfg(not(target_arch = "wasm32"))]
async fn poll_loop<C>(
    inner: std::sync::Weak<Inner<C>>,
    interval: Duration,
    cancel: CancellationToken,
) where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = rt::sleep(interval) => {}
        }
        let Some(handle) = inner.upgrade() else {
            return;
        };
        if let Err(error) = poll_once(&handle).await {
            tracing::warn!(%error, "configuration revision poll failed");
        }
    }
}

/// The fast path: react to a peer's notification within milliseconds. A closed
/// topic is resubscribed with a 1s→30s backoff, and every fresh subscription
/// opens with `ResyncRequired`, so whatever was published while we were away
/// is recovered by the reload that follows.
#[cfg(not(target_arch = "wasm32"))]
async fn subscription_loop<C>(
    inner: std::sync::Weak<Inner<C>>,
    cache: Arc<dyn Cache>,
    cancel: CancellationToken,
) where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let mut backoff = MIN_BACKOFF;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match cache.subscribe(INVALIDATION_TOPIC).await {
            Ok(mut subscription) => loop {
                let received = tokio::select! {
                    _ = cancel.cancelled() => return,
                    received = subscription.recv() => received,
                };
                match received {
                    Ok(notification) => {
                        backoff = MIN_BACKOFF;
                        let Some(handle) = inner.upgrade() else {
                            return;
                        };
                        if let Err(error) = apply(&handle, notification).await {
                            tracing::warn!(%error, "invalidation could not be applied");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "invalidation subscription closed; resubscribing");
                        break;
                    }
                }
            },
            Err(error) => tracing::warn!(%error, "invalidation subscription refused; retrying"),
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = rt::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}
