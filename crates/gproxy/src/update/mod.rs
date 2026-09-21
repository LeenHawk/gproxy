//! Self-update: check on a schedule, install only when asked.
//!
//! # The rule this module exists to enforce
//!
//! **A gateway holding a pile of upstream credentials does not replace its own
//! binary on its own.** Finding a new version is a report; installing one is
//! an explicit act — `gproxy update` typed by an operator, or a button in the
//! console. There is a switch for fully automatic installation
//! ([`UpdateOptions::automatic`]), it is off unless somebody turns it on, and
//! what it trades away is written out wherever it is mentioned.
//!
//! So the two verbs are separate all the way down: [`Updater::check_now`]
//! downloads a manifest and nothing else, and [`Updater::apply_now`] is the
//! only function in this crate that writes an executable.
//!
//! # What is trusted, and in what order
//!
//! ```text
//! manifest  →  ed25519 signature  →  channel  →  data-version floor
//!           →  artifact for this target  →  size  →  sha256
//!           →  zip entry named `gproxy`  →  backup  →  atomic rename
//! ```
//!
//! Every step refuses rather than continues, and every step before the
//! download costs nothing, so an instance that cannot take this release finds
//! out for the price of a few kilobytes. The signature is checked before any
//! *field* of the manifest is read as an instruction — [`manifest::Manifest`]
//! has no unverified constructor — and the hash before the archive is parsed,
//! because a zip parser fed arbitrary network bytes is an attack surface and
//! one fed the exact bytes a signed manifest named is not.
//!
//! The verification key is compiled in, from `GPROXY_UPDATE_PUBKEY`. It is the
//! one value here with no flag and no runtime variable, deliberately: an
//! operator who can choose the key can point the instance at their own
//! manifest. See [`config`].
//!
//! # Where it is not
//!
//! Not in `gproxy-host-axum`. That crate compiles for
//! `wasm32-unknown-unknown` as well, because the Workers host mounts its
//! router, and a module that downloads files and renames one over
//! `/proc/self/exe` cannot go there. The host declares a trait
//! ([`gproxy_host_axum::UpdateService`]); this module implements it; the
//! routes exist only in a process that supplied one.
//!
//! # What has never been proven
//!
//! There has never been a v4 release, so **no signed manifest exists for a
//! real version** and the end-to-end path against one is unproven. Everything
//! else is exercised: a generated key pair, a manifest signed with it, a fake
//! artifact served over loopback, and every refusal — see `tests/update.rs`.

pub mod config;
mod download;
mod extract;
mod manifest;
mod notes;
mod signature;
mod swap;
mod version;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use gproxy_host_axum::{AppliedUpdate, UpdateFailure, UpdateReport, UpdateSchedule, UpdateService};

pub use config::{
    BUILD_CHANNEL, BUILD_HASH, BUILD_VERSION, Channel, DEFAULT_INTERVAL_SECS, Restart, UpdateError,
    UpdateOptions,
};
pub use version::DATA_VERSION;

use crate::{Result, Settings};

/// This module's own result.
///
/// [`UpdateError`] rather than [`crate::Error`], because the taxonomy is what
/// decides the HTTP status a console sees; the command line flattens it into
/// `crate::Error` at the one place it prints.
type Outcome<T> = std::result::Result<T, UpdateError>;

/// How long after startup the first scheduled check runs.
///
/// Not zero: a process that has just bound a socket should answer requests
/// before it makes one, and an instance restarted in a loop by a broken
/// supervisor should not turn into a poll of the manifest host. Not the full
/// interval either — an operator who restarts to pick up a configuration
/// change wants to know about a release today, not in six hours.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(60);

/// The subdirectory of the data directory an artifact is unpacked into.
const STAGING_DIR: &str = ".update";

/// One instance's updater: the configuration, an HTTP client, and the last
/// thing the schedule found.
pub struct Updater {
    client: reqwest::Client,
    /// Where an artifact is staged before it is renamed into place. Under the
    /// data directory rather than `/tmp`, for two reasons: a rename across
    /// filesystems is a copy and cannot be atomic, and a small `/tmp` is the
    /// most common way an update of a few tens of megabytes fails.
    staging: PathBuf,
    options: UpdateOptions,
    recorded: Mutex<Recorded>,
    /// The scheduled check, aborted when this value is dropped. The task holds
    /// a `Weak` back, so the two do not keep each other alive.
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[derive(Default)]
struct Recorded {
    last_check: Option<UpdateReport>,
    last_error: Option<String>,
}

impl Drop for Updater {
    fn drop(&mut self) {
        if let Some(task) = self
            .task
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            task.abort();
        }
    }
}

impl std::fmt::Debug for Updater {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Updater")
            .field("options", &self.options)
            .field("staging", &self.staging)
            .finish_non_exhaustive()
    }
}

impl Updater {
    /// Build an updater that stages artifacts under `data_dir`.
    pub fn new(data_dir: &Path, options: UpdateOptions) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            client: download::client()?,
            staging: data_dir.join(STAGING_DIR),
            options,
            recorded: Mutex::new(Recorded::default()),
            task: Mutex::new(None),
        }))
    }

    /// The same, from a whole [`Settings`], resolving the data directory the
    /// way [`crate::instance`] resolves everything else relative to it.
    pub fn for_settings(settings: &Settings, options: UpdateOptions) -> Result<Arc<Self>> {
        let data_dir = settings
            .config
            .data_dir
            .clone()
            .unwrap_or_else(|| crate::config::DEFAULT_DATA_DIR.to_owned());
        Self::new(Path::new(&data_dir), options)
    }

    pub fn options(&self) -> &UpdateOptions {
        &self.options
    }

    /// Start the scheduled check, if there is one to start.
    ///
    /// Idempotent and cheap: with no interval configured it spawns nothing at
    /// all, so an operator who switched checking off pays for no timer.
    ///
    /// This is a plain task rather than a third synchronization loop on
    /// purpose. [`gproxy_app::App`] already owns two loops, and both exist to
    /// keep a *snapshot* in step with the database; a version check shares
    /// nothing with them — not the trigger, not the interval, not the failure
    /// handling, and not the thing being kept fresh. Hanging it off them would
    /// couple a network poll of a release host to the correctness of every
    /// request's configuration.
    pub fn start(self: &Arc<Self>) {
        let Some(interval) = self.options.interval_secs else {
            tracing::debug!(
                "scheduled update checks are switched off; `gproxy update --check` still works"
            );
            return;
        };
        if self.options.automatic {
            tracing::warn!(
                "GPROXY_UPDATE_AUTOMATIC is on: this instance will download and install a new \
                 release by itself and then {} — on a release nobody read the notes for, at an \
                 hour nobody chose, interrupting whatever was in flight",
                match self.options.restart {
                    Restart::None => "wait for a restart",
                    Restart::Supervisor => "exit for its supervisor to restart",
                    Restart::ReExec => "replace its own process image",
                }
            );
        }
        let weak = Arc::downgrade(self);
        let handle = tokio::spawn(async move {
            tokio::time::sleep(FIRST_CHECK_DELAY).await;
            let period = Duration::from_secs(interval);
            loop {
                // Upgraded inside the loop and dropped before the sleep, so a
                // shutdown between two checks is not waited on.
                match weak.upgrade() {
                    Some(updater) => updater.scheduled_check().await,
                    None => return,
                }
                tokio::time::sleep(period).await;
            }
        });
        *self.task.lock().unwrap_or_else(|error| error.into_inner()) = Some(handle);
    }

    /// One scheduled check: record it, say something if there is news, and
    /// install only if the operator asked for that.
    async fn scheduled_check(self: &Arc<Self>) {
        let channel = self.options.channel;
        let report = match self.fetch_manifest(channel).await {
            Ok(manifest) => self.report(channel, &manifest).await,
            Err(error) => Err(error),
        };
        match report {
            Ok(report) => {
                let available = report.available;
                let latest = report.latest.clone();
                let notes = report.notes_url.clone();
                self.record(Ok(report));
                if !available {
                    tracing::debug!(channel = channel.as_str(), "no new release");
                    return;
                }
                tracing::info!(
                    version = %latest,
                    channel = channel.as_str(),
                    notes = notes.as_deref().unwrap_or("—"),
                    "a new GPROXY release is available; install it with `gproxy update` or from \
                     the console. Nothing has been downloaded."
                );
                if self.options.automatic {
                    match self.apply_now(None, true).await {
                        Ok(applied) => tracing::warn!(
                            version = applied.version.as_deref().unwrap_or("?"),
                            restart = %applied.restart,
                            "installed a release automatically, as configured"
                        ),
                        Err(error) => tracing::error!(%error, "automatic update failed"),
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, channel = channel.as_str(), "update check failed");
                self.record(Err(&error));
            }
        }
    }

    /// Check now: fetch the manifest, verify it, and describe what it offers.
    /// Downloads no artifact and writes nothing.
    pub async fn check_now(&self, requested: Option<&str>) -> Outcome<UpdateReport> {
        let channel = self.channel(requested)?;
        let manifest = self.fetch_manifest(channel).await?;
        self.report(channel, &manifest).await
    }

    /// Install. The explicit act.
    ///
    /// `restart_after` is `false` on the command line, where there is nothing
    /// to restart — the process swaps the file and exits — and `true` from the
    /// HTTP surface and the automatic path, where a running server has to
    /// become the new binary somehow.
    pub async fn apply_now(
        &self,
        requested: Option<&str>,
        restart_after: bool,
    ) -> Outcome<AppliedUpdate> {
        let channel = self.channel(requested)?;
        let manifest = self.fetch_manifest(channel).await?;
        self.install(channel, &manifest, restart_after).await
    }

    /// Put the previous executable back.
    pub async fn rollback_now(&self, restart_after: bool) -> Outcome<AppliedUpdate> {
        swap::rollback()?;
        if restart_after {
            schedule_restart(self.options.restart);
        }
        Ok(AppliedUpdate {
            // Deliberately unnamed: the executable that was just put back
            // carries its own version, and nothing on disk records what it
            // was. Reporting this process's version would be a guess that is
            // wrong in exactly the common case — a console pressing rollback
            // is talking to the binary that was installed.
            version: None,
            changed: true,
            restart: self.options.restart.as_str().to_owned(),
        })
    }

    /// The channel in force: what the caller asked for, else what the instance
    /// was configured with.
    fn channel(&self, requested: Option<&str>) -> Outcome<Channel> {
        match requested {
            Some(value) => Channel::parse(value),
            None => Ok(self.options.channel),
        }
    }

    /// Download and verify the manifest, and refuse one that is not for the
    /// channel that was asked for.
    ///
    /// The channel check is not redundant with the signature: a signature
    /// proves the release publisher wrote this document, not that this
    /// document is the one for `release`. Without the check, a `beta` manifest
    /// placed at the `release` URL would be a validly signed instruction to
    /// install a release candidate.
    async fn fetch_manifest(&self, channel: Channel) -> Outcome<manifest::Manifest> {
        let url = self.options.manifest_url(channel);
        let manifest = download::manifest(&self.client, &url).await?;
        if manifest.channel != channel.as_str() {
            return Err(UpdateError::WrongChannel {
                expected: channel.as_str(),
                found: manifest.channel,
            });
        }
        Ok(manifest)
    }

    async fn report(
        &self,
        channel: Channel,
        manifest: &manifest::Manifest,
    ) -> Outcome<UpdateReport> {
        let target = version::target();
        // Looked up even when nothing is newer: a platform this release has no
        // build for must say so on a *check*, rather than at the moment an
        // operator presses apply.
        manifest.artifact(&target)?;
        let (current, available) = version::available(channel, &manifest.version)?;
        // Fetched only when there is something to read about, and never on
        // `dev`, where every rolling build has the same empty story.
        let notes = match manifest.notes_url.as_deref() {
            Some(url) if available && channel != Channel::Dev => {
                notes::fetch(&self.client, url).await
            }
            _ => None,
        };
        Ok(UpdateReport {
            current,
            latest: manifest.version.clone(),
            available,
            channel: channel.as_str().to_owned(),
            target,
            notes_url: manifest.notes_url.clone(),
            notes,
            restart: self.options.restart.as_str().to_owned(),
            rollback_available: swap::rollback_available(),
            checked_at_ms: now_ms(),
        })
    }

    /// The write half: the data-version gate, the download, the unpack, the
    /// swap.
    async fn install(
        &self,
        channel: Channel,
        manifest: &manifest::Manifest,
        restart_after: bool,
    ) -> Outcome<AppliedUpdate> {
        // Before anything is downloaded. An instance whose database this
        // release cannot open must not spend eighteen megabytes finding out.
        version::compatible(manifest.min_compatible_data_version, DATA_VERSION)?;
        let (_, available) = version::available(channel, &manifest.version)?;
        if !available {
            return Ok(AppliedUpdate {
                version: Some(manifest.version.clone()),
                changed: false,
                restart: Restart::None.as_str().to_owned(),
            });
        }
        let target = version::target();
        let artifact = manifest.artifact(&target)?;
        let bytes = download::artifact(&self.client, artifact).await?;
        let staged = extract::binary(&bytes, &self.staging)?;
        swap::install(&staged)?;
        // The staged copy is now redundant — `swap` copied it into place — and
        // it is a whole executable. Removing it is a courtesy, not a
        // correctness requirement, so its failure is not the operator's
        // problem.
        let _ = std::fs::remove_file(&staged);
        if restart_after {
            schedule_restart(self.options.restart);
        }
        Ok(AppliedUpdate {
            version: Some(manifest.version.clone()),
            changed: true,
            restart: self.options.restart.as_str().to_owned(),
        })
    }

    /// Record what a check found, so the console can read it without egress.
    ///
    /// A success clears the previous failure and a failure keeps the previous
    /// success: a console should be able to say "this is the newest release we
    /// know of, and we have not been able to reach the manifest since
    /// Tuesday", which needs both halves.
    fn record(&self, outcome: std::result::Result<UpdateReport, &UpdateError>) {
        let mut recorded = self
            .recorded
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match outcome {
            Ok(report) => {
                recorded.last_check = Some(report);
                recorded.last_error = None;
            }
            Err(error) => recorded.last_error = Some(error.to_string()),
        }
    }
}

impl UpdateService for Updater {
    fn recorded(&self) -> UpdateSchedule {
        let recorded = self
            .recorded
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        UpdateSchedule {
            last_check: recorded.last_check.clone(),
            last_error: recorded.last_error.clone(),
            interval_secs: self.options.interval_secs,
            automatic: self.options.automatic,
        }
    }

    fn check(
        &self,
        channel: Option<String>,
    ) -> gproxy_host_axum::update::UpdateFuture<'_, UpdateReport> {
        Box::pin(async move {
            let result = self.check_now(channel.as_deref()).await;
            // A manual check is the freshest thing the console has, so it
            // replaces what the schedule recorded rather than sitting beside
            // it.
            match &result {
                Ok(report) => self.record(Ok(report.clone())),
                Err(error) => self.record(Err(error)),
            }
            result.map_err(UpdateFailure::from)
        })
    }

    fn apply(
        &self,
        channel: Option<String>,
    ) -> gproxy_host_axum::update::UpdateFuture<'_, AppliedUpdate> {
        Box::pin(async move {
            self.apply_now(channel.as_deref(), true)
                .await
                .map_err(UpdateFailure::from)
        })
    }

    fn rollback(&self) -> gproxy_host_axum::update::UpdateFuture<'_, AppliedUpdate> {
        Box::pin(async move { self.rollback_now(true).await.map_err(UpdateFailure::from) })
    }
}

/// `gproxy update` and `gproxy update --check`.
///
/// `--check` prints and exits. Without it, the same report is printed **first**
/// and then the install happens, because an operator who typed `gproxy update`
/// and got a version number they did not expect should read it before the
/// executable moves.
///
/// Nothing restarts: this process swapped a file and is about to exit, and the
/// binary on disk takes effect at the next start. A running server is what the
/// HTTP surface and [`Restart`] are for.
pub async fn run(
    settings: &Settings,
    options: UpdateOptions,
    check: bool,
    channel: Option<String>,
) -> Result<()> {
    let updater = Updater::for_settings(settings, options)?;
    let selected = updater.channel(channel.as_deref())?;
    let manifest = updater.fetch_manifest(selected).await?;
    let report = updater.report(selected, &manifest).await?;
    print_report(&report);

    if check {
        return Ok(());
    }
    if !report.available {
        println!("\nAlready up to date; nothing was downloaded.");
        return Ok(());
    }

    println!(
        "\nInstalling {} for {}.\n  The running executable is backed up next to itself as \
         `gproxy.prev`, and `gproxy update --rollback` is not needed to undo it — the console's \
         rollback, or moving that file back, is.\n  This process does not restart: the new binary \
         takes effect the next time gproxy starts.",
        report.latest, report.target
    );
    let applied = updater.install(selected, &manifest, false).await?;
    match (&applied.version, applied.changed) {
        (Some(version), true) => println!("\nInstalled {version}. Restart gproxy to run it."),
        (Some(version), false) => println!("\nAlready on {version}; nothing was written."),
        (None, _) => println!("\nDone."),
    }
    Ok(())
}

/// What `--check` prints. Written for a terminal, not for a parser: a
/// deployment that wants to branch on this calls `GET /admin/api/update`,
/// which is the same data as JSON.
fn print_report(report: &UpdateReport) {
    println!("GPROXY {}", report.current);
    println!("  channel:  {}", report.channel);
    println!("  target:   {}", report.target);
    println!(
        "  latest:   {}{}",
        report.latest,
        if report.available {
            "  (newer than this build)"
        } else {
            "  (this build is current)"
        }
    );
    if let Some(url) = report.notes_url.as_deref() {
        println!("  notes:    {url}");
    }
    if report.rollback_available {
        println!("  rollback: a previous executable is on disk");
    }
    if let Some(notes) = report.notes.as_deref() {
        println!("\n{notes}");
    }
}

/// Bring the process onto the executable that is now on disk.
///
/// A quarter of a second late, so the HTTP response that asked for this has
/// been written before the process goes away. There is no graceful drain: an
/// operator who pressed apply asked for exactly this, and a drain that waits
/// for a streaming completion could hold the old binary for minutes.
fn schedule_restart(restart: Restart) {
    if matches!(restart, Restart::None) {
        tracing::info!(
            "the new executable is installed; this process keeps running the old one until it is \
             restarted"
        );
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match restart {
            Restart::None => {}
            // 42, as in v3, so a supervisor's logs distinguish "updated" from
            // a crash.
            Restart::Supervisor => std::process::exit(42),
            Restart::ReExec => reexec(),
        }
    });
}

/// Replace this process image with the new executable, same arguments.
///
/// `exec` rather than spawn-and-exit: the PID, the parent, the controlling
/// terminal and every inherited descriptor stay as they were, so a supervisor
/// that is watching this PID does not notice a restart it was not asked to
/// perform.
#[cfg(unix)]
fn reexec() -> ! {
    use std::os::unix::process::CommandExt as _;
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            tracing::error!(%error, "cannot locate this executable to re-exec it");
            std::process::exit(1)
        }
    };
    // Only returns on failure.
    let error = std::process::Command::new(executable)
        .args(std::env::args_os().skip(1))
        .exec();
    tracing::error!(%error, "re-exec of the updated executable failed");
    std::process::exit(1)
}

/// Windows has no `exec`, so the supervisor path is the only one available.
#[cfg(not(unix))]
fn reexec() -> ! {
    std::process::exit(42)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn updater(options: UpdateOptions) -> Arc<Updater> {
        let directory = tempfile::tempdir().unwrap();
        Updater::new(directory.path(), options).unwrap()
    }

    #[test]
    fn a_fresh_updater_has_checked_nothing_and_says_so() {
        let updater = updater(UpdateOptions::default());
        let schedule = updater.recorded();
        assert!(schedule.last_check.is_none());
        assert!(schedule.last_error.is_none());
        assert!(!schedule.automatic);
        assert_eq!(schedule.interval_secs, Some(DEFAULT_INTERVAL_SECS));
    }

    #[test]
    fn a_failure_does_not_erase_the_last_good_answer() {
        let updater = updater(UpdateOptions::default());
        let report = UpdateReport {
            current: "4.0.0".into(),
            latest: "4.1.0".into(),
            available: true,
            channel: "release".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            notes_url: None,
            notes: None,
            restart: "re-exec".into(),
            rollback_available: false,
            checked_at_ms: 1,
        };
        updater.record(Ok(report));
        updater.record(Err(&UpdateError::Download));

        let schedule = updater.recorded();
        assert_eq!(schedule.last_check.unwrap().latest, "4.1.0");
        assert_eq!(
            schedule.last_error.as_deref(),
            Some("update download failed")
        );
    }

    #[test]
    fn a_later_success_clears_the_failure() {
        let updater = updater(UpdateOptions::default());
        updater.record(Err(&UpdateError::Download));
        assert!(updater.recorded().last_error.is_some());
        updater.record(Ok(UpdateReport {
            current: "4.0.0".into(),
            latest: "4.0.0".into(),
            available: false,
            channel: "release".into(),
            target: "t".into(),
            notes_url: None,
            notes: None,
            restart: "none".into(),
            rollback_available: false,
            checked_at_ms: 2,
        }));
        assert!(updater.recorded().last_error.is_none());
    }

    #[test]
    fn the_requested_channel_beats_the_configured_one_and_a_bad_one_is_refused() {
        let updater = updater(UpdateOptions {
            channel: Channel::Release,
            ..UpdateOptions::default()
        });
        assert_eq!(updater.channel(None).unwrap(), Channel::Release);
        assert_eq!(updater.channel(Some("beta")).unwrap(), Channel::Beta);
        let error = updater.channel(Some("nightly")).unwrap_err();
        assert!(
            error.to_string().contains("not an update channel"),
            "{error}"
        );
    }

    /// Nothing is spawned when checking is off, so an operator who switched it
    /// off pays for no timer and makes no request.
    #[tokio::test]
    async fn no_interval_starts_no_task() {
        let updater = updater(UpdateOptions {
            interval_secs: None,
            ..UpdateOptions::default()
        });
        updater.start();
        assert!(
            updater
                .task
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_scheduled_task_is_aborted_when_the_updater_is_dropped() {
        let updater = updater(UpdateOptions {
            // Long enough that the first check never runs during the test.
            interval_secs: Some(3600),
            ..UpdateOptions::default()
        });
        updater.start();
        let handle = updater
            .task
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map(|task| task.abort_handle())
            .expect("a task was spawned");
        assert!(!handle.is_finished());
        drop(updater);
        // `abort` is asynchronous; yield until the runtime has run the
        // cancellation rather than asserting on a race.
        for _ in 0..100 {
            if handle.is_finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(handle.is_finished(), "the task outlived its updater");
    }

    #[test]
    fn the_staging_directory_is_under_the_data_directory() {
        // Not `/tmp`: a rename across filesystems is a copy and cannot be
        // atomic, and a small `/tmp` is the usual way a 20 MB update fails.
        let settings = Settings {
            config: gproxy_app::AppConfig {
                data_dir: Some("/var/lib/gproxy".into()),
                ..gproxy_app::AppConfig::default()
            },
            admin: Default::default(),
            telemetry: Default::default(),
            instance_id: None,
        };
        let updater = Updater::for_settings(&settings, UpdateOptions::default()).unwrap();
        assert_eq!(
            updater.staging,
            PathBuf::from("/var/lib/gproxy").join(STAGING_DIR)
        );
    }

    /// The contract the trait's `Send` bound asks for. A compile-time
    /// assertion rather than a runtime one: if a future here stopped being
    /// `Send` — by holding the state lock across an await, say — the router
    /// would stop compiling somewhere far from the cause.
    #[test]
    fn the_service_futures_are_send() {
        fn assert_send<T: Send>(_: T) {}
        let updater = updater(UpdateOptions::default());
        assert_send(updater.check(None));
        assert_send(updater.apply(None));
        assert_send(updater.rollback());
        fn assert_sync<T: Send + Sync>(_: &T) {}
        assert_sync(&*updater);
    }
}
