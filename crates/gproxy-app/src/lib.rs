//! The GPROXY v4 product layer: who is calling, what they may do, and the
//! operations that change that. It owns identity (users, gateway API keys,
//! organizations, teams, permissions, rate limits, the OAuth
//! issuer, audit), admission, and the typed admin/portal/issuer operations.
//!
//! What it deliberately does not own: an HTTP framework, a router, a runtime,
//! a CLI, and every engine concern (routing, credential selection, failover,
//! protocol conversion, settlement). Hosts adapt transports to the types here;
//! `gproxy-core` and `gproxy-sdk` execute. That is what keeps this crate
//! buildable for both a native server and `wasm32-unknown-unknown`.
//!
//! The planned request shape is `Caller` → `Admitted` → the sdk's `call()`:
//! authentication produces a caller, admission turns it into an allowed
//! provider set, an allowed credential set, a budget owner chain, a scope and
//! a session identity, and the sdk executes with exactly those. [`App`] is
//! where those pieces meet: it holds the handle, the configuration and the
//! identity snapshot, and exposes the data plane ([`App::call`],
//! [`App::connect`]), the vendor service views ([`App::call_service`]) and the
//! publication routes.
//!
//! There are two management surfaces on top of that, and they are shaped
//! differently on purpose. [`Operations`] is the operator's: every family
//! takes an id and acts on whatever row it names, and who may call it is the
//! host's decision. [`Portal`] is the end user's: it is built from a `Caller`,
//! no method on it takes a user id, and an id that belongs to somebody else
//! answers `NotFound` rather than `Forbidden`.

mod error;
pub use error::AppError;

mod hex;
mod rt;

pub mod config;
pub use config::AppConfig;

pub mod snapshot;
pub use snapshot::{AppData, AppSnapshot};

pub mod admission;
pub use admission::{Admission, AdmissionRequest, Admitted};

pub mod admin_scope;
pub use admin_scope::{AdminAdmission, AdminScope, SCOPE_HEADER};

pub mod admin_surface;
pub use admin_surface::{ADMIN_SECTIONS, AdminSection, SectionScope, require_section};

pub mod audit;
pub use audit::{Audit, AuditEntry};

pub mod auth;
pub use auth::{Authenticator, Caller, CallerKind, GrantContext};

pub mod call;
pub use call::{CallOutcome, ConnectOutcome, DataPlaneRequest};

pub mod capture;
pub use capture::{
    CaptureDirection, CaptureOutcome, CapturedFrame, DownstreamCapture, ObservationSwitches,
};

pub mod dto;
pub mod operations;
pub use operations::{Issuer, IssuerError, IssuerOrigin, Operations, Portal, Scope, ScopedManage};

pub mod publication;
pub use publication::AppPublicationUrl;

pub mod service;
pub use service::{RequestedView, ServiceRequestIn};

pub mod sync;
pub use sync::{DEFAULT_POLL_INTERVAL, INVALIDATION_TOPIC, SyncMode};

use gproxy_sdk::Gproxy;
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::{Arc, OnceLock};

pub type Result<T> = std::result::Result<T, AppError>;

/// The engine handle, as [`App`] has to hold it on each target.
///
/// Natively this is the handle itself. On `wasm32-unknown-unknown` the engine
/// below is deliberately `!Send`: a JS transport handle belongs to the isolate
/// that made it, and `gproxy-client` says so in its type
/// (`ClientBounds` is `Send + Sync` natively and empty on wasm). That is the
/// right statement about the engine and the wrong one about *this* type, which
/// a host has to be able to put in a request-scoped state — and axum's
/// `Router<S>` asks for `S: Clone + Send + Sync + 'static` before it will hold
/// one.
///
/// A Worker isolate is single-threaded, so the wrapper's promise — this value
/// is only ever touched from the thread that made it — is one the runtime
/// keeps. It is checked at runtime rather than asserted: a cross-thread poll
/// panics instead of racing.
#[cfg(not(target_arch = "wasm32"))]
type Handle<C> = Gproxy<C>;
#[cfg(target_arch = "wasm32")]
type Handle<C> = send_wrapper::SendWrapper<Gproxy<C>>;

#[cfg(not(target_arch = "wasm32"))]
fn hold<C>(gproxy: Gproxy<C>) -> Handle<C> {
    gproxy
}
#[cfg(target_arch = "wasm32")]
fn hold<C>(gproxy: Gproxy<C>) -> Handle<C> {
    send_wrapper::SendWrapper::new(gproxy)
}

/// One configured instance: the engine handle, the instance configuration and
/// the identity snapshot, assembled once and shared by every request.
///
/// Cheap to share — the handle is an `Arc` inside — and every method takes
/// `&self`, so a host keeps one of these for the process (or, at the edge, for
/// the isolate) and hands out references.
///
/// # Two snapshots, one revision
///
/// The handle publishes `CoreData` (providers, credentials, routes, prices)
/// and this type publishes [`AppData`] (users, keys, permissions, limits).
/// They are separate because they are read by different layers, and they are
/// kept in step by being refreshed from the same durable revision:
/// [`App::reload_all`] does both. Within one request the rule is the same as
/// everywhere else in this crate — take the snapshot once, hold it for the
/// whole request — which is why the decision surfaces below take it as an
/// argument rather than loading it per call. Two loads in one request could
/// straddle a reload and disagree about who the caller is.
///
/// # Nothing rebuilds itself
///
/// An identity write commits and notifies; it does not reload — see
/// [`operations`]. So an instance that never refreshes serves the identity it
/// started with, and a key minted a second ago cannot authenticate against it.
/// [`sync`] is the other half: [`App::sync_now`] on the host's write path,
/// [`App::start_sync`] for the peers' writes, [`App::tick`] where there is no
/// task to run a loop in.
pub struct App<C> {
    gproxy: Handle<C>,
    config: Arc<AppConfig>,
    snapshot: AppSnapshot,
    /// Serializes the reads that rebuild [`AppData`], so a burst of writes
    /// costs one reload rather than one per write, and a slow load cannot
    /// interleave its publication with a fast one's.
    refresh_lock: tokio::sync::Mutex<()>,
    /// Set once, by [`App::start_sync`]: the loops need a `Weak` to the `Arc`
    /// a host holds, which does not exist until the host has made one.
    sync: OnceLock<sync::AppSync>,
}

impl<C> App<C> {
    /// Wrap an assembled handle. The identity snapshot starts empty — every
    /// key is unknown and every permission absent — until the first
    /// [`App::refresh`]; a host calls [`App::reload_all`] once at startup and
    /// then [`App::start_sync`].
    pub fn new(gproxy: Gproxy<C>, config: AppConfig) -> Self {
        Self {
            gproxy: hold(gproxy),
            config: Arc::new(config),
            snapshot: AppSnapshot::default(),
            refresh_lock: tokio::sync::Mutex::new(()),
            sync: OnceLock::new(),
        }
    }

    /// The engine handle. Management writes, logins, queries and the raw
    /// `call`/`connect` builders live there; everything this crate adds is a
    /// decision made before one of them runs.
    pub fn gproxy(&self) -> &Gproxy<C> {
        &self.gproxy
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// The configuration as a shared pointer, for a host that keeps request
    /// state of its own.
    pub fn config_arc(&self) -> &Arc<AppConfig> {
        &self.config
    }

    /// The identity snapshot in force. **Hold the returned `Arc` for the whole
    /// request** and pass it to [`App::authenticator`] and
    /// [`App::admission`]; a second load could land on the other side of a
    /// reload.
    pub fn data(&self) -> Arc<AppData> {
        self.snapshot.load()
    }

    /// The published snapshot itself, for a host that wants its revision
    /// without loading it.
    pub fn snapshot(&self) -> &AppSnapshot {
        &self.snapshot
    }

    /// Authentication against `data`.
    ///
    /// The snapshot is a parameter rather than something this reads itself
    /// because the returned value borrows it for as long as it lives, and the
    /// published snapshot can be replaced at any moment. Taking it explicitly
    /// is also the request-scoped rule written down: one load, one request.
    pub fn authenticator<'a>(&'a self, data: &'a AppData) -> Authenticator<'a, C> {
        Authenticator::new(self.gproxy.store(), data, &self.config)
    }

    /// Admission against `data`, the cache the handle shares with its peers
    /// and this instance's configuration. Same snapshot rule as
    /// [`App::authenticator`].
    pub fn admission<'a>(&'a self, data: &'a AppData) -> Admission<'a, C> {
        Admission::new(self.gproxy.store(), data, self.gproxy.cache(), &self.config)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// Re-read identity and publish it, answering with the revision that was
    /// read.
    ///
    /// Serialized: two callers racing — a notification and a poll for the same
    /// write, a write path and a background loop — do one read after the
    /// other rather than two at once. The publication is monotonic on top of
    /// that, so a load that finishes after a newer one changes nothing, and a
    /// read that fails leaves the previous snapshot serving. The returned
    /// revision is what was *read*, which is not necessarily what is now
    /// active.
    pub async fn refresh(&self) -> Result<i64> {
        let _guard = self.refresh_lock.lock().await;
        let all = self.gproxy.store().load_all_data().await?;
        // The settings row carries the revision every write bumps. Its absence
        // is a broken installation, not revision zero: the builder creates it.
        let revision = all
            .control
            .settings
            .as_ref()
            .map(|settings| settings.config_revision)
            .ok_or_else(|| AppError::internal("the global settings row is missing"))?;
        let mut data = AppData::assemble(revision, &all.identity, &all.control.credentials)?;
        // The logging switches come out of the same read as the identity rows,
        // so a request that pinned this snapshot captures under the policy of
        // the revision it was decided under.
        data.observation = ObservationSwitches::from_settings(all.control.settings.as_ref());
        data.settings = all.control.settings;
        self.snapshot.publish_if_newer(Arc::new(data));
        Ok(revision)
    }

    /// Reload the engine's snapshot and then this one, so the two layers agree
    /// about what exists.
    ///
    /// This is two reads, not one: `Core::reload_data` assembles from its own
    /// batch. A write landing between them leaves the engine one revision
    /// behind identity rather than ahead of it, which is the harmless order —
    /// a key that can reach a provider the engine has not loaded yet resolves
    /// to no target, whereas the reverse would be a permission decided against
    /// providers the caller was never granted.
    pub async fn reload_all(&self) -> Result<i64> {
        self.gproxy.reload().await?;
        self.refresh().await
    }
}

impl<C> std::fmt::Debug for App<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("revision", &self.snapshot.revision())
            .finish_non_exhaustive()
    }
}

/// Dropping the last handle ends the synchronization loops at their next wake
/// even though they only hold a `Weak`: without this they would sleep out a
/// whole poll interval first. The same reason `gproxy-sdk`'s `Inner` has one.
impl<C> Drop for App<C> {
    fn drop(&mut self) {
        if let Some(sync) = self.sync.get() {
            sync.cancel();
        }
    }
}

/// Wall clock in milliseconds since the Unix epoch, saturating rather than
/// panicking on a clock the platform reports as before it. `web-time` gives
/// the same call `Date.now()` semantics inside a Worker isolate.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
