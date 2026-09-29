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
/// The whole path, driven against a manifest the test signs itself.
#[cfg(test)]
mod e2e;
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

use gproxy_host_axum::{
    AppliedUpdate, UpdateFailure, UpdateProgress, UpdateReport, UpdateSchedule, UpdateService,
};

pub use config::{
    BUILD_CHANNEL, BUILD_HASH, BUILD_VERSION, Channel, DEFAULT_INTERVAL_SECS, Restart, Source,
    UpdateError, UpdateOptions,
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
    /// The file an apply replaces, resolved **once at construction**.
    ///
    /// `None` when this process cannot locate itself, which on every platform
    /// this ships to means something has already gone wrong; checking still
    /// works and installing is refused with a message rather than a panic.
    /// Resolving it here rather than at apply time means an operator learns
    /// about it in the startup log, not while watching a progress bar.
    executable: Option<PathBuf>,
    /// The ed25519 key every manifest is verified against. Initialised from
    /// [`config::SIGNING_PUBLIC_KEY`], which is compiled in and has no runtime
    /// source — see that module for why the trust root is not configurable.
    signing_key: Option<String>,
    options: UpdateOptions,
    runtime: Mutex<Option<(Channel, Source, bool)>>,
    recorded: Mutex<Recorded>,
    progress: Mutex<Option<UpdateProgress>>,
    /// The scheduled check, aborted when this value is dropped. The task holds
    /// a `Weak` back, so the two do not keep each other alive.
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

/// Clears the live status on success, failure, or cancellation.
struct InstallProgress<'a>(&'a Mutex<Option<UpdateProgress>>);

impl<'a> InstallProgress<'a> {
    fn start(cell: &'a Mutex<Option<UpdateProgress>>, total: u64) -> Outcome<Self> {
        let mut current = cell.lock().unwrap_or_else(|error| error.into_inner());
        if current.is_some() {
            return Err(UpdateError::Configuration(
                "an update is already in progress".into(),
            ));
        }
        *current = Some(UpdateProgress {
            phase: "downloading".into(),
            downloaded_bytes: 0,
            total_bytes: total,
        });
        Ok(Self(cell))
    }

    fn publish(&self, phase: &str, downloaded: u64) {
        if let Some(current) = self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
        {
            current.phase = phase.into();
            current.downloaded_bytes = downloaded;
        }
    }
}

impl Drop for InstallProgress<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }
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
            .field("executable", &self.executable)
            // Never the key. It is public key material and harmless, but a
            // `Debug` that prints a trust root teaches the wrong habit.
            .field("signed", &self.signing_key.is_some())
            .finish_non_exhaustive()
    }
}

impl Updater {
    /// Build an updater that stages artifacts under `data_dir` and replaces
    /// the executable this process is running.
    pub fn new(data_dir: &Path, options: UpdateOptions) -> Result<Arc<Self>> {
        let executable = match std::env::current_exe() {
            Ok(path) => Some(path),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "this process cannot locate its own executable; update checks still work but \
                     installing one will be refused"
                );
                None
            }
        };
        if config::SIGNING_PUBLIC_KEY.is_none() {
            tracing::debug!(
                "no update signing key was compiled into this binary (GPROXY_UPDATE_PUBKEY), so \
                 every manifest will be refused; this is the normal state of a source build"
            );
        }
        Ok(Arc::new(Self {
            client: download::client()?,
            staging: data_dir.join(STAGING_DIR),
            executable,
            signing_key: config::SIGNING_PUBLIC_KEY.map(str::to_owned),
            options,
            runtime: Mutex::new(None),
            recorded: Mutex::new(Recorded::default()),
            progress: Mutex::new(None),
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

    /// An updater that verifies against a key the caller generated and
    /// replaces a file the caller named.
    ///
    /// Test-only, and it is the only way the real path can be exercised at
    /// all: the production key is compiled in from the build environment, a
    /// checkout has none, and the production target is the test harness's own
    /// binary. Nothing here weakens a released build — the two values it
    /// overrides have exactly one production source each, a few lines above.
    #[cfg(test)]
    pub(crate) fn for_test(
        data_dir: &Path,
        executable: &Path,
        signing_key: Option<String>,
        options: UpdateOptions,
    ) -> Arc<Self> {
        Arc::new(Self {
            client: download::client().expect("an HTTP client"),
            staging: data_dir.join(STAGING_DIR),
            executable: Some(executable.to_owned()),
            signing_key,
            options,
            runtime: Mutex::new(None),
            recorded: Mutex::new(Recorded::default()),
            progress: Mutex::new(None),
            task: Mutex::new(None),
        })
    }

    pub fn configure(
        self: &Arc<Self>,
        channel: Option<&str>,
        source: Option<&str>,
        enabled: bool,
    ) -> Outcome<()> {
        let channel = channel
            .map(Channel::parse)
            .transpose()?
            .unwrap_or(self.options.channel);
        let source = source
            .map(Source::parse)
            .transpose()?
            .unwrap_or(self.options.source);
        let mut runtime = self.runtime.lock().unwrap();
        if *runtime == Some((channel, source, enabled)) {
            return Ok(());
        }
        *runtime = Some((channel, source, enabled));
        drop(runtime);
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
        self.start();
        Ok(())
    }

    fn interval(&self) -> Option<u64> {
        match *self.runtime.lock().unwrap() {
            Some((_, _, true)) => Some(self.options.interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS)),
            Some((_, _, false)) => None,
            None => self.options.interval_secs,
        }
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
        let Some(interval) = self.interval() else {
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
        let channel = self.channel(None).expect("configured channel");
        let source = self.source(None).expect("configured source");
        let report = match self.fetch_manifest(channel, source).await {
            Ok(manifest) => self.report(channel, source, &manifest).await,
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
                    match self.apply_now(None, None, true).await {
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
    pub async fn check_now(
        &self,
        requested: Option<&str>,
        requested_source: Option<&str>,
    ) -> Outcome<UpdateReport> {
        let channel = self.channel(requested)?;
        let source = self.source(requested_source)?;
        let manifest = self.fetch_manifest(channel, source).await?;
        self.report(channel, source, &manifest).await
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
        requested_source: Option<&str>,
        restart_after: bool,
    ) -> Outcome<AppliedUpdate> {
        let channel = self.channel(requested)?;
        let source = self.source(requested_source)?;
        let manifest = self.fetch_manifest(channel, source).await?;
        self.install(channel, &manifest, restart_after).await
    }

    /// Download a verified Tauri APK for Android's package installer.
    /// The app has its own artifact key: the legacy `-apk` entries package the
    /// server and have a different Android application id.
    pub async fn stage_apk(&self) -> Outcome<Option<PathBuf>> {
        let channel = self.channel(None).expect("configured channel");
        let source = self.source(None).expect("configured source");
        let manifest = self.fetch_manifest(channel, source).await?;
        version::compatible(manifest.min_compatible_data_version, DATA_VERSION)?;
        let target = format!("{}-tauri-apk", version::target());
        let artifact = manifest.artifact(&target)?;
        if !version::available(channel, &manifest.version)?.1 {
            return Ok(None);
        }
        let progress = InstallProgress::start(&self.progress, artifact.size)?;
        let bytes = download::artifact(&self.client, artifact, |phase, bytes| {
            progress.publish(phase, bytes);
        })
        .await?;
        progress.publish("installing", bytes.len() as u64);
        std::fs::create_dir_all(&self.staging)
            .map_err(|error| UpdateError::io("creating the APK staging directory", error))?;
        let marker = self.staging.join("install-apk.pending");
        if marker.exists() {
            std::fs::remove_file(&marker)
                .map_err(|error| UpdateError::io("clearing the previous APK marker", error))?;
        }
        let pending = self.staging.join("gproxy-update.apk.tmp");
        let apk = self.staging.join("gproxy-update.apk");
        std::fs::write(&pending, bytes)
            .map_err(|error| UpdateError::io("staging the verified APK", error))?;
        std::fs::rename(&pending, &apk)
            .map_err(|error| UpdateError::io("publishing the verified APK", error))?;
        std::fs::write(marker, manifest.version)
            .map_err(|error| UpdateError::io("marking the APK ready to install", error))?;
        Ok(Some(apk))
    }

    /// Put the previous executable back.
    pub async fn rollback_now(&self, restart_after: bool) -> Outcome<AppliedUpdate> {
        swap::rollback(self.executable()?)?;
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
            None => Ok(self
                .runtime
                .lock()
                .unwrap()
                .map_or(self.options.channel, |(channel, _, _)| channel)),
        }
    }

    fn source(&self, requested: Option<&str>) -> Outcome<Source> {
        match requested {
            Some(value) => Source::parse(value),
            None => Ok(self
                .runtime
                .lock()
                .unwrap()
                .map_or(self.options.source, |(_, source, _)| source)),
        }
    }

    /// The file an apply replaces, or a refusal an operator can act on.
    ///
    /// Only the two writing paths call this. A check does not, so an instance
    /// that cannot locate its own executable still reports new releases — it
    /// simply cannot install one.
    fn executable(&self) -> Outcome<&Path> {
        self.executable.as_deref().ok_or_else(|| {
            UpdateError::Configuration(
                "this process cannot locate its own executable, so there is nothing to replace"
                    .to_owned(),
            )
        })
    }

    /// Download and verify the manifest, and refuse one that is not for the
    /// channel that was asked for.
    ///
    /// The channel check is not redundant with the signature: a signature
    /// proves the release publisher wrote this document, not that this
    /// document is the one for `release`. Without the check, a `beta` manifest
    /// placed at the `release` URL would be a validly signed instruction to
    /// install a release candidate.
    async fn fetch_manifest(
        &self,
        channel: Channel,
        source: Source,
    ) -> Outcome<manifest::Manifest> {
        let url = self.options.manifest_url(channel, source);
        let manifest = download::manifest(&self.client, &url, self.signing_key.as_deref()).await?;
        if channel == Channel::Release && manifest.channel == "releases" {
            return Err(UpdateError::Configuration(
                "this source still offers a v3 stable release; select the dev channel for v4 preview builds".into(),
            ));
        }
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
        source: Source,
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
            source: source.as_str().to_owned(),
            target,
            notes_url: manifest.notes_url.clone(),
            notes,
            restart: self.options.restart.as_str().to_owned(),
            rollback_available: self
                .executable
                .as_deref()
                .is_some_and(swap::rollback_available),
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
        let progress = InstallProgress::start(&self.progress, artifact.size)?;
        let bytes = download::artifact(&self.client, artifact, |phase, bytes| {
            progress.publish(phase, bytes);
        })
        .await?;
        progress.publish("installing", bytes.len() as u64);
        let staged = extract::binary(&bytes, &self.staging)?;
        swap::install(self.executable()?, &staged)?;
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
    fn progress(&self) -> Option<UpdateProgress> {
        self.progress
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn recorded(&self) -> UpdateSchedule {
        let recorded = self
            .recorded
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        UpdateSchedule {
            last_check: recorded.last_check.clone(),
            last_error: recorded.last_error.clone(),
            interval_secs: self.interval(),
            automatic: self.options.automatic,
            channel: self
                .channel(None)
                .expect("configured channel")
                .as_str()
                .into(),
            source: self
                .source(None)
                .expect("configured source")
                .as_str()
                .into(),
        }
    }

    fn check(
        &self,
        channel: Option<String>,
        source: Option<String>,
    ) -> gproxy_host_axum::update::UpdateFuture<'_, UpdateReport> {
        Box::pin(async move {
            let result = self.check_now(channel.as_deref(), source.as_deref()).await;
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
        source: Option<String>,
    ) -> gproxy_host_axum::update::UpdateFuture<'_, AppliedUpdate> {
        Box::pin(async move {
            self.apply_now(channel.as_deref(), source.as_deref(), true)
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
    let source = updater.source(None)?;
    let manifest = updater.fetch_manifest(selected, source).await?;
    let report = updater.report(selected, source, &manifest).await?;
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
    println!("  source:   {}", report.source);
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
            source: "github".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            notes_url: None,
            notes: None,
            restart: "re-exec".into(),
            rollback_available: false,
            checked_at_ms: 1,
        };
        updater.record(Ok(report));
        updater.record(Err(&UpdateError::Download("test failure".into())));

        let schedule = updater.recorded();
        assert_eq!(schedule.last_check.unwrap().latest, "4.1.0");
        assert_eq!(
            schedule.last_error.as_deref(),
            Some("update download failed: test failure")
        );
    }

    #[test]
    fn a_later_success_clears_the_failure() {
        let updater = updater(UpdateOptions::default());
        updater.record(Err(&UpdateError::Download("test failure".into())));
        assert!(updater.recorded().last_error.is_some());
        updater.record(Ok(UpdateReport {
            current: "4.0.0".into(),
            latest: "4.0.0".into(),
            available: false,
            channel: "release".into(),
            source: "github".into(),
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
        assert_send(updater.check(None, None));
        assert_send(updater.apply(None, None));
        assert_send(updater.rollback());
        fn assert_sync<T: Send + Sync>(_: &T) {}
        assert_sync(&*updater);
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[tokio::test]
    async fn settings_can_stop_and_restart_scheduled_checks() {
        let dir = tempfile::tempdir().unwrap();
        let updater = Updater::new(dir.path(), UpdateOptions::default()).unwrap();
        updater.configure(Some("beta"), Some("cnb"), false).unwrap();
        assert_eq!(updater.channel(None).unwrap(), Channel::Beta);
        assert_eq!(updater.source(None).unwrap(), Source::Cnb);
        assert_eq!(updater.source(Some("github")).unwrap(), Source::Github);
        assert!(updater.source(Some("other")).is_err());
        assert_eq!(updater.recorded().source, "cnb");
        assert!(updater.interval().is_none());
        updater
            .configure(Some("dev"), Some("github"), true)
            .unwrap();
        assert_eq!(updater.channel(None).unwrap(), Channel::Dev);
        assert_eq!(updater.source(None).unwrap(), Source::Github);
        assert!(updater.task.lock().unwrap().is_some());
        updater.configure(None, None, false).unwrap();
        assert!(updater.task.lock().unwrap().is_none());
    }
}
