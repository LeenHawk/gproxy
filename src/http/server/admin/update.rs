//! §19.10 self-update admin endpoints (native-only):
//!   GET  /admin/update/check  — manifest fetch, returns CheckReport.
//!   GET  /admin/update/status — in-process status state machine snapshot.
//!   POST /admin/update/apply  — download + verify + swap (Restart::None; stage only).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;

use crate::api::error::ApiError;
use crate::app::AppState;
use crate::app::update_status::UpdateStatus;
use crate::selfupdate::{self, Channel, CheckReport, Restart, UpdateContext, UpdateError};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an `UpdateContext` from the runtime config, or fail with Conflict if
/// `update_repo` is not configured.
fn context(state: &AppState) -> Result<UpdateContext, ApiError> {
    let repo = state.config.update_repo.clone().ok_or_else(|| {
        ApiError::Conflict("self-update is not configured (set --update-repo)".to_string())
    })?;

    let channel = match state.config.update_channel.as_str() {
        "staging" => Channel::Staging,
        _ => Channel::Releases,
    };

    Ok(UpdateContext {
        repo,
        channel,
        data_dir: state.config.update_data_dir.clone(),
        client: Arc::clone(&state.upstream),
    })
}

/// Map `UpdateError` to an `ApiError`. Exhaustive over every variant.
fn update_error(e: UpdateError) -> ApiError {
    let msg = e.to_string();
    match e {
        // Availability / config problems → 400 Bad Request.
        UpdateError::NoArtifact(_) | UpdateError::Version(_) => ApiError::BadRequest(msg),
        // Policy refusals (compat floor, downgrade guard) → 409 Conflict.
        UpdateError::Incompatible(_) | UpdateError::Downgrade(_) => ApiError::Conflict(msg),
        // I/O and pipeline failures → 500 Internal.
        UpdateError::Manifest(_)
        | UpdateError::Download(_)
        | UpdateError::Integrity(_)
        | UpdateError::Signature(_)
        | UpdateError::Io(_)
        | UpdateError::Swap(_) => ApiError::Internal(msg),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /admin/update/check` — fetch manifest and report availability.
/// Pure read; does NOT mutate the status state machine.
pub async fn check(State(state): State<AppState>) -> Result<Json<CheckReport>, ApiError> {
    let ctx = context(&state)?;
    selfupdate::check(&ctx)
        .await
        .map(Json)
        .map_err(update_error)
}

/// `GET /admin/update/status` — snapshot of the in-process status state machine.
pub async fn status(State(state): State<AppState>) -> Json<UpdateStatus> {
    let snapshot = state
        .update_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(snapshot)
}

/// `POST /admin/update/apply` — download, verify, and atomically swap the
/// binary (stage only: `Restart::None` so the response is sent).
///
/// # Single-flight guard
/// If a check/apply is already in flight (`Checking` | `Downloading`), returns
/// 409 rather than starting a second concurrent run.
///
/// # Mutex discipline
/// The `std::sync::Mutex` guard is dropped **before** the `.await` to avoid
/// holding a `!Send` lock across a suspension point (`clippy::await_holding_lock`).
/// Pattern: lock → read/write → drop guard → await → lock → write terminal state → drop.
///
/// # Why `Restart::None`?
/// `Supervisor` and `ReExec` both terminate the process (`→ !`). With either
/// of those the HTTP response would never be sent. `None` stages the new binary
/// and returns; the operator restarts the process at their own schedule.
pub async fn apply(State(state): State<AppState>) -> Result<Json<UpdateStatus>, ApiError> {
    // Fail fast on bad config before touching the status machine (no lock held).
    let ctx = context(&state)?;

    // --- single-flight guard: atomic check-and-set under one lock ---
    // Check and the `Downloading` write happen in the SAME lock scope, so two
    // concurrent applies can't both pass the guard (no TOCTOU).
    {
        let mut guard = state
            .update_status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match *guard {
            UpdateStatus::Checking | UpdateStatus::Downloading => {
                return Err(ApiError::Conflict(
                    "an update is already in progress".to_string(),
                ));
            }
            _ => {}
        }
        *guard = UpdateStatus::Downloading;
    } // guard dropped here — before the await below

    // --- async work (no lock held) ---
    let result = selfupdate::apply(&ctx, Restart::None).await;

    // --- set terminal state (lock scope 3) ---
    // Convert result → (terminal status, api outcome) without partial moves.
    let (terminal, api_result) = match result {
        Ok(version) => {
            let s = UpdateStatus::Staged {
                version: version.clone(),
            };
            (s.clone(), Ok(Json(s)))
        }
        Err(e) => {
            let s = UpdateStatus::Failed {
                error: e.to_string(),
            };
            (s, Err(update_error(e)))
        }
    };
    {
        let mut guard = state
            .update_status
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = terminal;
    }

    api_result
}
