//! Keeping the identity snapshot in step with the database — the other half of
//! the contract [`operations`](crate::operations) writes down.
//!
//! An operation commits and notifies; it does not reload. Something has to,
//! and that something is here: one implementation, driven by all three hosts,
//! because an [`AppData`](crate::AppData) that never gets rebuilt means a key
//! created a moment ago cannot authenticate, a permission that was granted is
//! still refused, and a membership that exists reads as "administers nothing"
//! — in the very process that wrote the row.
//!
//! # Two mechanisms, two questions
//!
//! **"Did *I* just write this?"** is answered by [`App::sync_now`], on the
//! host's write path. It reads `settings.config_revision` and rebuilds when
//! that number is ahead of the snapshot being served. It is the only mechanism
//! that can promise read-your-writes, and the reason is the cache: a
//! `MemoryCache` publish reaches a subscriber in the same process
//! synchronously, but a Redis publish is a round trip and the subscriber
//! connection learns about it whenever it learns about it. A notification is
//! therefore never on the critical path of the caller's next request; the
//! durable number is.
//!
//! **"Did somebody else?"** is answered by the two loops below: a subscription
//! on [`INVALIDATION_TOPIC`], which is fast and lossy, and a poll of the same
//! durable revision, which is slow and cannot miss anything. A notification
//! carries no state — it only says "look again".
//!
//! This is `gproxy-sdk`'s `sync` module applied to the other snapshot, and
//! deliberately the same discipline: the loops hold a [`Weak`](std::sync::Weak)
//! so a dropped instance ends them, reloads are serialized by
//! [`App::refresh`]'s own lock, publication is monotonic, a failed reload
//! leaves the previous snapshot serving, and a closed topic is resubscribed
//! with a bounded backoff. It is a second implementation rather than a shared
//! one because the two snapshots are rebuilt by different code from different
//! rows; what they share is the rulebook, not the reload.
//!
//! # Scopes
//!
//! A write names the families it touched, and the names travel as opaque
//! strings — this crate's [`Scope`](crate::operations::Scope) and the sdk's
//! `manage::Scope` publish into the same list. A notification about a routing
//! table has nothing to tell identity, so it is ignored; anything else,
//! including a name this build has never heard of and a payload that will not
//! parse at all, refreshes. Unknown means reload: always correct, merely
//! slower.
//!
//! # No loop at the edge
//!
//! A Worker isolate does not outlive its request, so a spawned loop would be
//! cancelled mid-sleep and never poll. [`App::tick`] is the whole mechanism
//! there, called at the top of a request beside the sdk's own — exactly as the
//! sdk documents its wasm story.

use std::{sync::Arc, time::Duration};

use gproxy_cache::{Cache, Notification, Subscription};
use gproxy_core::Invalidation;
use gproxy_seaorm::BatchConnectionTrait;
use tokio_util::sync::CancellationToken;

use crate::{App, AppError, Result, rt};

/// The topic invalidations travel on. Core's constant, re-exported so a host
/// that publishes its own names the same string.
pub use gproxy_core::keys::INVALIDATION_TOPIC;

/// Who drives synchronization. The sdk's switch, reused rather than mirrored:
/// a host configures one instance, and one answer has to serve both layers of
/// it.
pub use gproxy_sdk::SyncMode;

/// The default revision poll interval, the same number the sdk uses. The poll
/// is the backstop, not the mechanism — a deployment that notices a
/// thirty-second lag is one whose notifications are not arriving.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Resubscription backoff after the notification topic closes or refuses.
/// Only the background loop resubscribes; `tick` leaves a closed topic to the
/// revision poll of the same call.
#[cfg(not(target_arch = "wasm32"))]
const MIN_BACKOFF: Duration = Duration::from_secs(1);
#[cfg(not(target_arch = "wasm32"))]
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// The scope names that cannot change [`AppData`](crate::AppData), and so are
/// the only ones a notification may be ignored for.
///
/// All of them are `gproxy-sdk`'s: providers, routes, profiles, rewrite rules,
/// endpoint rules, quotas, prices and model catalogues are the engine's view,
/// and `credential_state` is a secret, an expiry or a lifecycle status — never
/// the owner columns [`CredentialOwnership`](crate::snapshot::CredentialOwnership)
/// is built from. `credentials` and `settings` are deliberately **not** here:
/// the first can move an owner, and the second carries the observation
/// switches a snapshot pins.
///
/// The list is an exclusion rather than an inclusion so that it is safe when
/// it is wrong: a scope nobody here recognises — a name the sdk grows next
/// week — refreshes.
const UNRELATED: [&str; 9] = [
    "providers",
    "routing",
    "profiles",
    "rewrite",
    "endpoints",
    "quotas",
    "pricing",
    "models",
    "credential_state",
];

/// What an instance keeps in order to stay in step: the loops' cancellation,
/// and — when nothing is looping — the subscription `tick` drains.
pub(crate) struct AppSync {
    cancel: CancellationToken,
    /// Only `Manual` keeps one. In `Background` the loop owns its own, and a
    /// second subscriber here would race it for the same notifications.
    subscription: Option<tokio::sync::Mutex<Box<dyn Subscription>>>,
}

impl AppSync {
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// Subscribe to the invalidation topic and start whatever loops this
    /// target and mode have.
    ///
    /// Called once, by the host, after the first [`App::reload_all`] — the
    /// order matters in `Manual` mode, where a fresh subscription always opens
    /// with `ResyncRequired` and that initial load *is* the resync.
    ///
    /// Takes `&Arc<Self>` because the loops hold a `Weak` to this very
    /// allocation. Calling it twice is a host bug and is refused with a
    /// warning rather than a second set of loops.
    pub async fn start_sync(self: &Arc<Self>, mode: SyncMode, poll_interval: Duration) {
        // wasm has no task to run a loop in, whatever the mode says.
        let manual = mode == SyncMode::Manual || cfg!(target_arch = "wasm32");
        let handle = AppSync {
            cancel: CancellationToken::new(),
            subscription: if manual {
                subscribe(self.gproxy().cache()).await
            } else {
                None
            },
        };
        if self.sync.set(handle).is_err() {
            tracing::warn!("identity synchronization is already running for this instance");
            return;
        }
        self.spawn_loops(manual, poll_interval);
    }

    #[cfg(target_arch = "wasm32")]
    fn spawn_loops(self: &Arc<Self>, _manual: bool, _poll_interval: Duration) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn spawn_loops(self: &Arc<Self>, manual: bool, poll_interval: Duration) {
        if manual {
            return;
        }
        let Some(sync) = self.sync.get() else {
            return;
        };
        let cancel = sync.cancel.clone();
        let polling = rt::spawn(poll_loop(
            Arc::downgrade(self),
            poll_interval,
            cancel.clone(),
        ));
        let listening = rt::spawn(subscription_loop(
            Arc::downgrade(self),
            self.gproxy().cache().clone(),
            cancel,
        ));
        if !polling || !listening {
            tracing::warn!(
                "no Tokio runtime for identity synchronization; build with SyncMode::Manual and \
                 call tick()"
            );
        }
    }

    /// One revision poll: rebuild the identity snapshot when the durable
    /// `settings.config_revision` is ahead of the one being served, otherwise
    /// do nothing and touch nothing else.
    ///
    /// This is what a host calls after a write it just performed, which is the
    /// only way a caller's very next request is guaranteed to see it. It is
    /// also the backstop that makes a lost notification cost a poll interval
    /// rather than correctness.
    ///
    /// Answers the revision it published, or `None` when it published nothing.
    pub async fn sync_now(&self) -> Result<Option<i64>> {
        let durable = self.durable_revision().await?;
        self.refresh_if_newer(durable).await
    }

    /// Apply the invalidations already waiting on the shared cache, without
    /// asking the database anything.
    ///
    /// Does nothing when this instance keeps no subscription of its own, which
    /// is every `Background` deployment: there the loop owns it.
    pub async fn drain_notifications(&self) -> Result<Option<i64>> {
        let Some(subscription) = self.sync.get().and_then(|sync| sync.subscription.as_ref()) else {
            return Ok(None);
        };
        let mut subscription = subscription.lock().await;
        let mut last = None;
        // Stops on the first "nothing pending" or on a closed topic; neither is
        // worth an error, because the revision poll of this same tick is the
        // authority and resubscription is the background loop's job.
        while let Some(Ok(notification)) = rt::ready_now(subscription.recv()).await {
            last = self.apply(notification).await?.or(last);
        }
        Ok(last)
    }

    /// One synchronization step for a host with no background loop: apply
    /// whatever is already waiting, then poll the revision.
    ///
    /// This is what an edge isolate calls at the top of a request, next to
    /// [`Gproxy::tick`](gproxy_sdk::Gproxy::tick). Natively, under
    /// `SyncMode::Background`, it is redundant but harmless.
    pub async fn tick(&self) -> Result<Option<i64>> {
        let drained = self.drain_notifications().await?;
        Ok(self.sync_now().await?.or(drained))
    }

    /// The durable revision, straight off the settings row. A missing row is a
    /// broken installation rather than revision zero: the builder creates it
    /// and every write needs it.
    pub(crate) async fn durable_revision(&self) -> Result<i64> {
        self.gproxy()
            .store()
            .settings()
            .get()
            .await?
            .map(|settings| settings.config_revision)
            .ok_or_else(|| AppError::internal("the global settings row is missing"))
    }

    /// Rebuild only when `revision` is ahead of what is being served.
    ///
    /// The comparison is a fast path, not the guarantee: [`App::refresh`]
    /// publishes through `AppSnapshot::publish_if_newer`, so a load that
    /// finishes out of order still cannot move the instance backwards.
    async fn refresh_if_newer(&self, revision: i64) -> Result<Option<i64>> {
        if revision <= self.snapshot().revision() {
            return Ok(None);
        }
        self.refresh().await.map(Some)
    }

    /// What one notification means here.
    ///
    /// Anything unreadable refreshes rather than guesses, and so does a scope
    /// name this build does not know: notifications are hints, and a wasted
    /// rebuild is cheap next to serving an identity that no longer exists.
    async fn apply(&self, notification: Notification) -> Result<Option<i64>> {
        let payload = match notification {
            // A fresh subscription, or one that lagged. Neither says anything
            // about revisions, so the durable state decides.
            Notification::ResyncRequired => return self.sync_now().await,
            Notification::Message(payload) => payload,
        };
        let Ok(invalidation) = serde_json::from_slice::<Invalidation>(&payload) else {
            tracing::warn!("unreadable invalidation payload; refreshing identity");
            return self.refresh().await.map(Some);
        };
        match invalidation {
            Invalidation::ConfigurationChanged { revision, scopes } => {
                if !touches_identity(&scopes) {
                    return Ok(None);
                }
                self.refresh_if_newer(clamp(revision.0)).await
            }
            // A rotated secret, a new expiry, a lifecycle status: the engine's
            // business. None of it is an owner column, and ownership is all
            // `AppData` keeps about a credential.
            Invalidation::CredentialChanged { .. } => Ok(None),
            // The row is gone, so its ownership entry has to go with it.
            Invalidation::CredentialRemoved { revision, .. } => {
                self.refresh_if_newer(clamp(revision.0)).await
            }
        }
    }

    /// Stop the background synchronization of both layers, engine first.
    ///
    /// The instance stays usable: `refresh`, `sync_now` and `tick` all still
    /// work, nothing reloads on its own any more.
    pub fn shutdown(&self) {
        self.gproxy().shutdown();
        if let Some(sync) = self.sync.get() {
            sync.cancel();
        }
    }
}

/// Subscribe and discard whatever is already queued.
///
/// A fresh subscription always opens with `ResyncRequired`; the host's initial
/// [`App::reload_all`] *is* that resync, so delivering it to the first `tick`
/// would only reload twice on every cold start — which a Worker pays for in
/// wall clock.
async fn subscribe(cache: &Arc<dyn Cache>) -> Option<tokio::sync::Mutex<Box<dyn Subscription>>> {
    match cache.subscribe(INVALIDATION_TOPIC).await {
        Ok(mut subscription) => {
            while let Some(Ok(_)) = rt::ready_now(subscription.recv()).await {}
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

/// Whether a write that named `scopes` could have changed `AppData`.
///
/// An empty list is a publisher that did not classify its write, which means
/// "anything" — core says so where the field is declared.
fn touches_identity(scopes: &[String]) -> bool {
    scopes.is_empty()
        || scopes
            .iter()
            .any(|scope| !UNRELATED.contains(&scope.as_str()))
}

/// A revision as this crate stores it. The wire carries `u64` and the column
/// is `i64`; a number past the signed range is not a real revision, and
/// saturating makes it compare as "ahead", which is the safe direction.
fn clamp(revision: u64) -> i64 {
    i64::try_from(revision).unwrap_or(i64::MAX)
}

/// The durable backstop: poll the revision, rebuild when it moved. Holds a
/// `Weak`, so dropping the last handle ends the loop at its next wake.
#[cfg(not(target_arch = "wasm32"))]
async fn poll_loop<C>(app: std::sync::Weak<App<C>>, interval: Duration, cancel: CancellationToken)
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    loop {
        if cancel
            .run_until_cancelled(rt::sleep(interval))
            .await
            .is_none()
        {
            return;
        }
        let Some(app) = app.upgrade() else {
            return;
        };
        if let Err(error) = app.sync_now().await {
            tracing::warn!(%error, "identity revision poll failed");
        }
    }
}

/// The fast path: react to a peer's write within milliseconds. A closed topic
/// is resubscribed with a 1s→30s backoff, and every fresh subscription opens
/// with `ResyncRequired`, so whatever landed while we were away is recovered
/// by the poll that follows.
#[cfg(not(target_arch = "wasm32"))]
async fn subscription_loop<C>(
    app: std::sync::Weak<App<C>>,
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
                let Some(received) = cancel.run_until_cancelled(subscription.recv()).await else {
                    return;
                };
                match received {
                    Ok(notification) => {
                        backoff = MIN_BACKOFF;
                        let Some(app) = app.upgrade() else {
                            return;
                        };
                        if let Err(error) = app.apply(notification).await {
                            tracing::warn!(%error, "invalidation could not be applied to identity");
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
        if cancel
            .run_until_cancelled(rt::sleep(backoff))
            .await
            .is_none()
        {
            return;
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unclassified_write_means_anything() {
        assert!(touches_identity(&[]));
    }

    #[test]
    fn the_engines_own_families_are_ignored() {
        for scope in UNRELATED {
            assert!(
                !touches_identity(&[scope.to_owned()]),
                "{scope} does not feed AppData"
            );
        }
    }

    #[test]
    fn identity_scopes_and_the_two_shared_ones_refresh() {
        for scope in [
            crate::operations::Scope::Identity.name(),
            crate::operations::Scope::Permissions.name(),
            crate::operations::Scope::Keys.name(),
            crate::operations::Scope::RateLimits.name(),
            crate::operations::Scope::Subscriptions.name(),
            crate::operations::Scope::OAuthClients.name(),
            // The sdk's, and both of them reach AppData: an owner column and
            // the observation switches.
            "credentials",
            "settings",
        ] {
            assert!(touches_identity(&[scope.to_owned()]), "{scope}");
        }
    }

    #[test]
    fn a_name_nobody_here_knows_refreshes_rather_than_guesses() {
        assert!(touches_identity(&["something-from-next-year".to_owned()]));
        // And one unknown name in a batch of ignorable ones is enough.
        assert!(touches_identity(&[
            "routing".to_owned(),
            "something-from-next-year".to_owned(),
        ]));
    }

    #[test]
    fn a_revision_past_the_signed_range_compares_as_ahead() {
        assert_eq!(clamp(7), 7);
        assert_eq!(clamp(u64::MAX), i64::MAX);
    }
}
