//! The self-update surface: a trait the host implements, and the five routes
//! that exist only when one did.
//!
//! # Why this is a trait and not a module
//!
//! Replacing the running executable means downloading a file, unpacking an
//! archive and renaming something over `/proc/self/exe`. None of that can live
//! in this crate: it compiles for `wasm32-unknown-unknown` as well, where
//! `gproxy-host-edge` mounts the same [`Router`] inside a Workers fetch
//! handler. A Worker has no executable to replace and no filesystem to stage
//! one on, so a module that did those things would not merely be dead code
//! there — it would not build.
//!
//! So this crate declares only the *shape* of the answer, exactly as
//! `gproxy-core` declares [`PublicationUrl`](gproxy_core::PublicationUrl) and
//! [`FetchPolicy`](gproxy_core::FetchPolicy) and lets its host supply the
//! behaviour. [`crates/gproxy`](https://github.com/LeenHawk/gproxy) — the only
//! native, process-owning layer — implements [`UpdateService`] and hands it to
//! [`HostState::with_updates`]. The edge and wasm builds hand over nothing, and
//! [`routes`] is never called there, so `/admin/api/update` does not exist at
//! the edge rather than existing and failing.
//!
//! **Nothing in this file may name a platform type.** No `Path`, no `File`, no
//! `Command`, no `std::process`. The trait carries strings, numbers and
//! booleans, which is all a console needs and all a Worker can compile.
//!
//! # The behavioural rule the surface encodes
//!
//! A gateway holding a pile of upstream credentials does not replace its own
//! binary on its own. Finding a new version is a *report*; installing it is an
//! explicit act. That is why there are two verbs rather than one:
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /admin/api/update` | the last scheduled check's result, from memory; no egress |
//! | `GET /admin/api/update/progress` | live download and installation progress; no egress |
//! | `POST /admin/api/update/check` | check now; still only a report |
//! | `POST /admin/api/update/apply` | download, verify, swap — the explicit act |
//! | `POST /admin/api/update/rollback` | put the previous executable back |
//!
//! [`UpdateSchedule::automatic`] is on the read so a console can say out loud
//! when an operator has turned the rule off.
//!
//! # Who may call it
//!
//! Instance scope only. The routes are mounted inside `/admin/api` and so sit
//! behind the same guard as everything else there, and each handler then calls
//! [`require_section`](gproxy_app::require_section) with `update` — a section
//! id `gproxy-app` does not declare, which is precisely how that function
//! spells "instance machinery": an organization or team administrator is
//! refused with a message that says so.

use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    Extension, Router,
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{AdminScope, require_section};
use gproxy_seaorm::BatchConnectionTrait;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{HostState, error::ErrorResponse};

/// The section id the handlers gate on. Undeclared in `gproxy_app`'s table on
/// purpose: [`require_section`] admits an unknown section to the instance
/// scope and nobody else, which is the rule this surface wants.
const SECTION: &str = "update";

/// A boxed answer from the host.
///
/// Boxed rather than an `async fn` in the trait because the trait has to be
/// `dyn`-compatible — the host stores one behind an `Arc<dyn UpdateService>` —
/// and `async fn` in a trait is not. `Send` is required on every target, which
/// costs the wasm build nothing: nothing implements this trait there.
pub type UpdateFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, UpdateFailure>> + Send + 'a>>;

/// What a host that owns its own executable can do about a new release.
///
/// Injected through [`HostState::with_updates`]. A host that supplies none
/// simply has no update routes.
pub trait UpdateService: Send + Sync {
    /// The last scheduled check, from memory. Cheap and synchronous: a console
    /// polls this, and a poll must not become egress.
    fn recorded(&self) -> UpdateSchedule;

    /// The current installation, without making an upstream request.
    fn progress(&self) -> Option<UpdateProgress> {
        None
    }

    /// Check now. Downloads and verifies the manifest; downloads no artifact
    /// and changes nothing on disk.
    fn check(
        &self,
        channel: Option<String>,
        source: Option<String>,
    ) -> UpdateFuture<'_, UpdateReport>;

    /// Download, verify and install. The explicit act.
    fn apply(
        &self,
        channel: Option<String>,
        source: Option<String>,
    ) -> UpdateFuture<'_, AppliedUpdate>;

    /// Put the executable this one replaced back where it was.
    fn rollback(&self) -> UpdateFuture<'_, AppliedUpdate>;
}

/// What the scheduled check has found so far, and whether there is a schedule.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateSchedule {
    /// The last completed check, or `None` when none has finished yet — a
    /// process that started a minute ago, or one whose every check failed.
    pub last_check: Option<UpdateReport>,
    /// Why the last check failed, when one did and none has since succeeded.
    /// A console shows this rather than an empty panel that looks like "no
    /// update available".
    pub last_error: Option<String>,
    /// Seconds between checks, or `None` when checking is switched off.
    pub interval_secs: Option<u64>,
    /// Whether this instance installs what it finds without being asked.
    /// **Off unless an operator turned it on**, and reported either way so a
    /// console can say which.
    pub automatic: bool,
    pub channel: String,
    pub source: String,
}

/// Live byte counts and the current installation phase.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateProgress {
    pub phase: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}

/// One check's answer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateReport {
    /// The version this process is running.
    pub current: String,
    /// The version the manifest offers for this channel.
    pub latest: String,
    /// Whether `latest` is actually newer than `current`.
    pub available: bool,
    pub channel: String,
    pub source: String,
    /// The artifact key the manifest was searched for, e.g.
    /// `x86_64-unknown-linux-gnu`.
    pub target: String,
    /// Where a human reads what changed. Straight from the manifest.
    pub notes_url: Option<String>,
    /// The notes themselves, when the host could fetch them. Never required:
    /// a release with no readable notes is still a release.
    pub notes: Option<String>,
    /// What the host will do to the process after an apply: `none`,
    /// `supervisor` or `re-exec`.
    pub restart: String,
    /// Whether a previous executable is still on disk to go back to.
    pub rollback_available: bool,
    /// When this check completed, in milliseconds since the epoch.
    pub checked_at_ms: i64,
}

/// The answer to an apply or a rollback.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppliedUpdate {
    /// The version now on disk.
    ///
    /// `None` after a **rollback**: the executable that was put back carries
    /// its own version and nothing on disk records what it was. Reporting the
    /// calling process's version instead would be a guess that is wrong in
    /// exactly the common case — a console pressing rollback is talking to the
    /// binary that was installed.
    pub version: Option<String>,
    /// Whether anything was actually replaced. `false` means the instance was
    /// already on that version and nothing was written.
    pub changed: bool,
    /// What is about to happen to the process.
    pub restart: String,
}

/// A refusal, in the three shapes a caller can act on differently.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct UpdateFailure {
    pub kind: UpdateFailureKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFailureKind {
    /// The operator configured something this host cannot use. Their mistake,
    /// fixable without touching the release.
    Configuration,
    /// The request was understood and refused: the release predates this
    /// instance's data, the version does not parse, there is nothing to roll
    /// back to.
    Refused,
    /// Something outside this process failed — the manifest host, the
    /// download, its signature, the archive, the filesystem. Not the caller's
    /// fault and not their problem to fix.
    Upstream,
}

impl UpdateFailure {
    pub fn configuration(message: impl Into<String>) -> Self {
        Self {
            kind: UpdateFailureKind::Configuration,
            message: message.into(),
        }
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            kind: UpdateFailureKind::Refused,
            message: message.into(),
        }
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self {
            kind: UpdateFailureKind::Upstream,
            message: message.into(),
        }
    }

    /// The stable string a client branches on, in the product envelope's
    /// `code` position.
    pub fn code(&self) -> &'static str {
        match self.kind {
            UpdateFailureKind::Configuration => "update_configuration",
            UpdateFailureKind::Refused => "update_refused",
            UpdateFailureKind::Upstream => "update_failed",
        }
    }

    fn status(&self) -> StatusCode {
        match self.kind {
            UpdateFailureKind::Configuration => StatusCode::BAD_REQUEST,
            UpdateFailureKind::Refused => StatusCode::CONFLICT,
            UpdateFailureKind::Upstream => StatusCode::BAD_GATEWAY,
        }
    }
}

impl std::fmt::Display for UpdateFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UpdateFailure {}

// The product envelope, built here rather than through `ErrorResponse`: an
// `AppError` that carried these would have to be `Internal`, and `Internal` is
// scrubbed to "the instance failed to process this request" on the way out.
// The whole value of "downloaded update failed its integrity check" is that the
// operator reads those words.
impl IntoResponse for UpdateFailure {
    fn into_response(self) -> Response {
        crate::error::json_response(
            self.status(),
            &serde_json::json!({
                "error": { "code": self.code(), "message": self.message }
            }),
        )
    }
}

/// `?channel=dev`, for an operator checking one channel without reconfiguring
/// the instance.
#[derive(Debug, Default, Deserialize)]
pub struct ChannelQuery {
    pub channel: Option<String>,
    pub source: Option<String>,
}

/// The five routes, to be merged into `/admin/api`.
///
/// Called **only** when [`HostState::updates`] is `Some`, so the surface does
/// not exist on a host that cannot update itself rather than existing and
/// answering 503 forever.
pub(crate) fn routes<C>() -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    Router::new()
        .route("/update", get(recorded::<C>))
        .route("/update/progress", get(progress::<C>))
        .route("/update/check", post(check::<C>))
        .route("/update/apply", post(apply::<C>))
        .route("/update/rollback", post(rollback::<C>))
}

async fn recorded<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let service = match instance_service(&state, &scope) {
            Ok(service) => service,
            Err(response) => return *response,
        };
        crate::error::ok_json(&service.recorded())
    })
    .await
}

async fn progress<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let service = match instance_service(&state, &scope) {
            Ok(service) => service,
            Err(response) => return *response,
        };
        crate::error::ok_json(&service.progress())
    })
    .await
}

async fn check<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<ChannelQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let service = match instance_service(&state, &scope) {
            Ok(service) => service,
            Err(response) => return *response,
        };
        match service.check(query.channel, query.source).await {
            Ok(report) => crate::error::ok_json(&report),
            Err(failure) => failure.into_response(),
        }
    })
    .await
}

async fn apply<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<ChannelQuery>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let service = match instance_service(&state, &scope) {
            Ok(service) => service,
            Err(response) => return *response,
        };
        match service.apply(query.channel, query.source).await {
            Ok(applied) => crate::error::ok_json(&applied),
            Err(failure) => failure.into_response(),
        }
    })
    .await
}

async fn rollback<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let service = match instance_service(&state, &scope) {
            Ok(service) => service,
            Err(response) => return *response,
        };
        match service.rollback().await {
            Ok(applied) => crate::error::ok_json(&applied),
            Err(failure) => failure.into_response(),
        }
    })
    .await
}

/// The scope gate and the service lookup, which every handler needs and none
/// may skip.
///
/// The `None` arm is unreachable through the router — [`routes`] is only
/// merged when a service exists — and is still written out, because "this
/// cannot happen" and `unwrap()` are not the same sentence in a function that
/// replaces an executable.
///
/// The failure is boxed because it is a whole rendered `Response`, and an
/// unboxed `Result` would make every `Ok` path carry its size.
fn instance_service<'a, C>(
    state: &'a HostState<C>,
    scope: &AdminScope,
) -> Result<&'a Arc<dyn UpdateService>, Box<Response>> {
    if let Err(error) = require_section(scope, SECTION) {
        return Err(Box::new(ErrorResponse(error).into_response()));
    }
    state.updates().ok_or_else(|| {
        Box::new(
            UpdateFailure::configuration("this host cannot update its own executable")
                .into_response(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_carries_the_status_a_caller_can_act_on() {
        for (failure, status, code) in [
            (
                UpdateFailure::configuration("no channel"),
                StatusCode::BAD_REQUEST,
                "update_configuration",
            ),
            (
                UpdateFailure::refused("older than this data"),
                StatusCode::CONFLICT,
                "update_refused",
            ),
            (
                UpdateFailure::upstream("signature verification failed"),
                StatusCode::BAD_GATEWAY,
                "update_failed",
            ),
        ] {
            assert_eq!(failure.status(), status, "{failure}");
            assert_eq!(failure.code(), code, "{failure}");
        }
    }

    /// The whole point of the type: the operator reads the words. A refusal
    /// rendered through `AppError::Internal` would say "the instance failed to
    /// process this request" instead.
    #[test]
    fn the_message_survives_into_the_body() {
        let response =
            UpdateFailure::upstream("downloaded update failed its integrity check").into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn a_schedule_that_has_never_run_says_so_rather_than_being_empty() {
        let schedule = UpdateSchedule::default();
        assert!(schedule.last_check.is_none());
        assert!(!schedule.automatic, "automatic must default to off");
        assert!(schedule.interval_secs.is_none());
    }
}
