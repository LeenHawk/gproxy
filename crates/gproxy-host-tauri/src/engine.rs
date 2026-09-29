//! One instance per process, assembled once, by whoever asks first.
//!
//! On the desktop this is a formality — the window is the only thing that ever
//! asks, and it asks in `setup`. On Android it is the whole arrangement:
//!
//! ```text
//!   the launcher        the boot receiver        a Stop tap
//!        │                      │                    │
//!   MainActivity          GproxyBootReceiver    the notification
//!        │                      └────────┬───────────┘
//!        │                        GproxyService (foreground)
//!        │                               │
//!    Tauri setup                    JNI nativeStart
//!        └───────────────┬───────────────┘
//!                 engine::ensure_started
//!                one Desktop · one App · one socket
//! ```
//!
//! Two callers, one instance. The activity and the service are different
//! entry points into the *same process*, and the phone's gateway is the
//! process rather than the window: a second assembly over the same SQLite file
//! would be two snapshots publishing independently and two background syncs
//! polling each other's writes, which is the thing [`crate::desktop`] refuses
//! to do across surfaces and must equally refuse to do across entry points.
//!
//! # Why the runtime is a process-global
//!
//! [`crate::run`] used to own its runtime as a local, which is correct when
//! `run()` only returns as the process exits. On Android it does not: the
//! activity can be destroyed — the user swipes the task away — while the
//! foreground service keeps the process alive, and the data plane's listener
//! and the instance's background sync have to outlive the window that happened
//! to start them. A runtime dropped at the end of `run()` would take them
//! with it, and a gateway that stops when you close the window is not a
//! gateway.
//!
//! # Why there is no restart
//!
//! [`ensure_started`] is once per process and there is no counterpart that
//! un-starts it. Stopping means ending the process — that is what the
//! notification's Stop action does, after [`shutdown`] has closed the socket
//! and stopped the sync — because the alternative is an `App` that Tauri has
//! already handed to two hundred and sixty-one commands as managed state going
//! stale underneath them. A cold process is also the only "restart" whose
//! behaviour is identical to a first launch, which is the one a person
//! reporting a bug will have.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use tokio::runtime::Runtime;

use crate::{Desktop, StartError, StartResult, secrets::SecretStore};

/// The runtime every instance in this process runs on.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// The instance, once there is one.
static ENGINE: OnceLock<Desktop> = OnceLock::new();

/// Serialises the assembly so that two entry points racing produce one
/// instance rather than two. Held across an `await`, which is why it is
/// tokio's mutex and not the standard library's.
static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The process's async runtime, built on first use.
///
/// Multi-threaded and Tokio's default width. It is deliberately *not*
/// `tauri::async_runtime`: Tauri's belongs to the Tauri app, and on Android
/// the instance has to outlive that.
pub fn runtime() -> StartResult<&'static Runtime> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let built =
        Runtime::new().map_err(|error| StartError::io("starting the async runtime", error))?;
    // Two threads reaching here at once build two runtimes and one of them is
    // dropped unused. That costs a handful of worker threads that never took
    // a task, and the alternative — a lock around the whole function — would
    // be a lock every caller takes forever to avoid a case that happens once.
    Ok(RUNTIME.get_or_init(|| built))
}

/// The instance rooted at `data_dir`, assembling it if this process has not
/// already.
///
/// `data_dir` and `store` are only consulted on the call that wins: a second
/// caller gets the instance that exists, whatever it was asked for. That is
/// the right answer rather than a compromise — there is one database per
/// process and the first caller is the one that named it.
pub async fn ensure_started(data_dir: &Path, store: &dyn SecretStore) -> StartResult<Desktop> {
    ensure_started_with_admin(data_dir, store, None).await
}

pub(crate) async fn ensure_started_with_admin(
    data_dir: &Path,
    store: &dyn SecretStore,
    admin: Option<gproxy::config::AdminOptions>,
) -> StartResult<Desktop> {
    if let Some(desktop) = ENGINE.get() {
        return Ok(desktop.clone());
    }
    let _gate = GATE.lock().await;
    // Checked again under the gate: the first check is the fast path for every
    // call after the first, and this one is the correctness check.
    if let Some(desktop) = ENGINE.get() {
        return Ok(desktop.clone());
    }
    let desktop = Desktop::start_with_admin(data_dir.to_path_buf(), store, admin).await?;
    Ok(ENGINE.get_or_init(|| desktop).clone())
}

/// The instance, if this process has one.
pub fn started() -> Option<&'static Desktop> {
    ENGINE.get()
}

/// Close the data plane's socket and stop the background sync.
///
/// Idempotent, and it does not un-start the instance: see the module note on
/// why stopping is ending the process.
pub fn shutdown() {
    if let Some(desktop) = ENGINE.get() {
        desktop.shutdown();
    }
}

/// The data directory a desktop instance uses, from Tauri's path resolver.
///
/// Android does not come through here: there the directory is the Java side's
/// to decide, because the service can be started by the boot receiver with no
/// activity and therefore no Tauri app to ask. See [`crate::android`].
#[cfg(desktop)]
pub fn data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> StartResult<PathBuf> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map_err(|error| StartError::App(gproxy_app::AppError::internal(error.to_string())))
}

/// OpenHarmony supplies the private files directory through the native Ability.
#[cfg(target_env = "ohos")]
pub fn data_dir<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> StartResult<PathBuf> {
    let ability = tauri::ohos::APP
        .lock()
        .map_err(|error| StartError::App(gproxy_app::AppError::internal(error.to_string())))?;
    ability
        .as_ref()
        .and_then(|app| app.base_path())
        .map(|path| PathBuf::from(path).join("gproxy"))
        .ok_or_else(|| {
            StartError::App(gproxy_app::AppError::internal(
                "OpenHarmony did not provide the application's private files directory",
            ))
        })
}

/// The data directory on Android: whatever the Java side configured.
#[cfg(target_os = "android")]
pub fn data_dir<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> StartResult<PathBuf> {
    crate::android::data_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_is_the_same_one_every_time() {
        let first = runtime().unwrap();
        let second = runtime().unwrap();
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn there_is_no_instance_before_anybody_starts_one() {
        // `ensure_started` is never called from the test suite — `tests/` uses
        // `Desktop::start` directly, so that two integration tests in one
        // process do not share an instance — and this asserts that the unit
        // tests have not quietly started depending on the global either.
        assert!(started().is_none());
        // And shutting down a process that has no instance is not an error.
        shutdown();
    }
}
