//! The GPROXY v4 product layer: who is calling, what they may do, and the
//! operations that change that. It owns identity (users, gateway API keys,
//! organizations, teams, permissions, subscriptions, rate limits, the OAuth
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

pub mod audit;
pub use audit::{Audit, AuditEntry};

pub mod auth;
pub use auth::{Authenticator, Caller, CallerKind, GrantContext};

pub mod call;
pub use call::{CallOutcome, ConnectOutcome, DataPlaneRequest};

pub mod capture;
pub mod dto;
pub mod operations;
pub use operations::{Operations, Scope};

pub mod publication;
pub use publication::AppPublicationUrl;

pub mod service;
pub use service::{RequestedView, ServiceRequestIn};

use gproxy_sdk::Gproxy;
use gproxy_seaorm::BatchConnectionTrait;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, AppError>;

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
pub struct App<C> {
    gproxy: Gproxy<C>,
    config: Arc<AppConfig>,
    snapshot: AppSnapshot,
}

impl<C> App<C> {
    /// Wrap an assembled handle. The identity snapshot starts empty — every
    /// key is unknown and every permission absent — until the first
    /// [`App::refresh`]; a host calls [`App::reload_all`] once at startup.
    pub fn new(gproxy: Gproxy<C>, config: AppConfig) -> Self {
        Self {
            gproxy,
            config: Arc::new(config),
            snapshot: AppSnapshot::default(),
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
    /// The publication is monotonic: a load that finishes after a newer one
    /// changes nothing, so a poll and a notification racing for the same write
    /// are harmless. The returned revision is what was *read*, which is not
    /// necessarily what is now active.
    pub async fn refresh(&self) -> Result<i64> {
        let all = self.gproxy.store().load_all_data().await?;
        // The settings row carries the revision every write bumps. Its absence
        // is a broken installation, not revision zero: the builder creates it.
        let revision = all
            .control
            .settings
            .as_ref()
            .map(|settings| settings.config_revision)
            .ok_or_else(|| AppError::internal("the global settings row is missing"))?;
        let data = Arc::new(AppData::assemble(
            revision,
            &all.identity,
            &all.control.credentials,
        )?);
        self.snapshot.publish_if_newer(data);
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

/// Wall clock in milliseconds since the Unix epoch, saturating rather than
/// panicking on a clock the platform reports as before it. `web-time` gives
/// the same call `Date.now()` semantics inside a Worker isolate.
pub(crate) fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
