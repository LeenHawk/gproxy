//! The whole update path, end to end, against a manifest this test signs and
//! an artifact it serves.
//!
//! # What this proves, and what it cannot
//!
//! It proves every step of the mechanism: a generated ed25519 key pair, a
//! manifest signed over the same payload `scripts/build-update-manifest.sh`
//! writes, a real zip served over a real loopback socket, a real
//! `rename(2)` over a real file. Check, apply, rollback, and each of the five
//! refusals — wrong key, wrong channel, wrong hash, a data floor above this
//! build, and a build with no key at all — go through the same functions a
//! release does.
//!
//! **It cannot prove the path against a real release**, because there has
//! never been a v4 one: no signed manifest exists for a real version, and the
//! release pipeline that would produce one has never run on v4. What is
//! unproven is therefore the *pipeline's* half of the contract — that the
//! manifest it writes has these field names, this payload byte layout and
//! artifacts at these URLs — not this code's half.
//!
//! # Why it is a unit test and not `tests/update.rs`
//!
//! Two of the values it has to control are deliberately not reachable from
//! outside the crate: the signing key, which production takes from a
//! compile-time constant, and the executable to replace, which production
//! takes from `std::env::current_exe()` — the test harness's own binary. An
//! integration test would need both of those to be public API, which would
//! make the trust root configurable at runtime. [`Updater::for_test`] is
//! `#[cfg(test)]` instead, and this is the module that can see it.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use axum::{Router, routing::get};
use sha2::{Digest as _, Sha256};

use super::{
    Channel, DATA_VERSION, Restart, UpdateError, UpdateOptions, Updater,
    config::BUILD_VERSION,
    download::hex,
    extract::fixture::archive,
    manifest::fixture::{Entry, manifest, public_key, signing_key},
    version,
};

/// The bytes the artifact's `gproxy` entry carries. Not a real executable:
/// what the swap has to get right is *which* bytes end up at the executable's
/// path, and a marker makes that unambiguous in a failure message.
const NEW_BINARY: &[u8] = b"the installed gproxy binary";
const OLD_BINARY: &[u8] = b"the gproxy binary that was running";

/// A signed manifest, an artifact and a notes document, served over loopback.
struct Release {
    base: String,
    /// How many times the artifact was actually fetched. The refusals that are
    /// supposed to happen *before* the download assert on this: an instance
    /// that refuses a release after spending eighteen megabytes has not
    /// refused it cheaply.
    artifact_hits: Arc<AtomicUsize>,
    directory: tempfile::TempDir,
    executable: PathBuf,
    public_key: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Release {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// How a test wants the release to differ from a correct one.
#[derive(Default)]
struct Broken {
    /// Sign with a key the updater does not trust.
    wrong_key: bool,
    /// Advertise a hash the bytes do not have.
    wrong_hash: bool,
    /// Claim a data floor this build cannot meet.
    future_data_version: bool,
    /// Say the manifest is for a channel other than the one asked for.
    channel: Option<&'static str>,
    /// Offer a version rather than the default `999.0.0`.
    version: Option<String>,
    /// Publish a build for a platform that is not this one.
    wrong_target: bool,
    tauri_apk: bool,
}

impl Release {
    async fn publish(broken: Broken) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the release fixture");
        let address: SocketAddr = listener.local_addr().expect("the fixture's address");
        let base = format!("http://{address}");

        let artifact = archive(&[("gproxy", NEW_BINARY)]);
        let sha = if broken.wrong_hash {
            hex(&Sha256::digest(b"some other archive entirely"))
        } else {
            hex(&Sha256::digest(&artifact))
        };
        let target = if broken.tauri_apk {
            format!("{}-tauri-apk", version::target())
        } else if broken.wrong_target {
            "sparc64-unknown-netbsd".to_owned()
        } else {
            version::target()
        };

        // The key the manifest is signed with, and the key the updater is told
        // to trust. Equal unless the test asked for them not to be.
        let key = signing_key(11);
        let trusted = public_key(&signing_key(if broken.wrong_key { 12 } else { 11 }));

        let document = manifest(
            &key,
            broken.channel.unwrap_or(Channel::RELEASE),
            broken.version.as_deref().unwrap_or("999.0.0"),
            Some(&format!("{base}/notes")),
            if broken.future_data_version {
                DATA_VERSION + 1
            } else {
                DATA_VERSION
            },
            &[Entry {
                target,
                url: format!("{base}/gproxy.zip"),
                sha256: sha,
                size: artifact.len() as u64,
            }],
        );

        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        let router = Router::new()
            .route("/manifest.json", get(move || async move { document }))
            .route(
                "/gproxy.zip",
                get(move || async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    artifact
                }),
            )
            // The GitHub releases API shape, which is what a `notes_url` in a
            // manifest this repository produced points at.
            .route(
                "/notes",
                get(|| async { r#"{"body":"  Faster settlement, fewer 502s.  "}"# }),
            );
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let directory = tempfile::tempdir().expect("a data directory");
        let executable = directory.path().join("gproxy");
        std::fs::write(&executable, OLD_BINARY).expect("the running executable");

        Self {
            base,
            artifact_hits: hits,
            directory,
            executable,
            public_key: trusted,
            server,
        }
    }

    fn updater(&self, options: UpdateOptions) -> Arc<Updater> {
        Updater::for_test(
            self.directory.path(),
            &self.executable,
            Some(self.public_key.clone()),
            UpdateOptions {
                manifest_url: Some(format!("{}/manifest.json", self.base)),
                ..options
            },
        )
    }

    /// The default: read the `release` channel, install nothing by itself,
    /// restart nothing — a test process must not re-exec or exit.
    fn default_updater(&self) -> Arc<Updater> {
        self.updater(UpdateOptions {
            channel: Channel::Release,
            restart: Restart::None,
            interval_secs: None,
            automatic: false,
            manifest_url: None,
        })
    }

    fn downloads(&self) -> usize {
        self.artifact_hits.load(Ordering::SeqCst)
    }

    fn on_disk(&self) -> Vec<u8> {
        std::fs::read(&self.executable).expect("the executable is still there")
    }

    fn previous(&self) -> Option<Vec<u8>> {
        std::fs::read(appended(&self.executable, ".prev")).ok()
    }
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

// ------------------------------------------------------------------ check --

#[tokio::test]
async fn a_check_reports_the_running_version_the_offer_and_the_notes() {
    let release = Release::publish(Broken::default()).await;
    let report = release
        .default_updater()
        .check_now(None)
        .await
        .expect("the check succeeds");

    assert_eq!(report.current, BUILD_VERSION);
    assert_eq!(report.latest, "999.0.0");
    assert!(report.available);
    assert_eq!(report.channel, "release");
    assert_eq!(report.target, version::target());
    assert_eq!(
        report.notes_url.as_deref(),
        Some(format!("{}/notes", release.base).as_str())
    );
    // Fetched and trimmed, because a console renders it next to a button.
    assert_eq!(
        report.notes.as_deref(),
        Some("Faster settlement, fewer 502s.")
    );
    assert_eq!(report.restart, "none");
    assert!(!report.rollback_available);
    assert!(report.checked_at_ms > 0);

    // A check is a report. It downloaded no artifact and wrote no file.
    assert_eq!(release.downloads(), 0);
    assert_eq!(release.on_disk(), OLD_BINARY);
}

#[tokio::test]
async fn a_check_records_itself_where_the_console_reads_it() {
    use gproxy_host_axum::UpdateService as _;

    let release = Release::publish(Broken::default()).await;
    let updater = release.default_updater();
    assert!(updater.recorded().last_check.is_none());

    updater.check(None).await.expect("the check succeeds");

    let schedule = updater.recorded();
    assert_eq!(schedule.last_check.expect("recorded").latest, "999.0.0");
    assert!(schedule.last_error.is_none());
    // And the two facts an operator needs to interpret it: there is no
    // schedule here, and this instance installs nothing on its own.
    assert_eq!(schedule.interval_secs, None);
    assert!(!schedule.automatic);
}

#[tokio::test]
async fn an_offer_that_is_not_newer_is_not_an_update() {
    let release = Release::publish(Broken {
        version: Some(BUILD_VERSION.to_owned()),
        ..Broken::default()
    })
    .await;
    let updater = release.default_updater();

    let report = updater.check_now(None).await.expect("the check succeeds");
    assert!(
        !report.available,
        "{} is not newer than itself",
        report.latest
    );

    // And applying it writes nothing rather than reinstalling the same bytes.
    let applied = updater.apply_now(None, false).await.expect("apply");
    assert!(!applied.changed);
    assert_eq!(applied.version.as_deref(), Some(BUILD_VERSION));
    assert_eq!(release.downloads(), 0);
    assert_eq!(release.on_disk(), OLD_BINARY);
    assert!(release.previous().is_none());
}

// --------------------------------------------------------------- refusals --

#[tokio::test]
async fn a_manifest_signed_by_another_key_is_refused_before_anything_else() {
    let release = Release::publish(Broken {
        wrong_key: true,
        ..Broken::default()
    })
    .await;
    let updater = release.default_updater();

    let error = updater.check_now(None).await.unwrap_err();
    assert!(matches!(error, UpdateError::Signature), "{error}");
    let error = updater.apply_now(None, false).await.unwrap_err();
    assert!(matches!(error, UpdateError::Signature), "{error}");

    assert_eq!(release.downloads(), 0);
    assert_eq!(release.on_disk(), OLD_BINARY);
}

#[tokio::test]
async fn a_build_with_no_signing_key_refuses_every_manifest() {
    let release = Release::publish(Broken::default()).await;
    // A source checkout: `GPROXY_UPDATE_PUBKEY` was never set, so there is
    // nothing to verify against. The answer is a refusal, never "accept it
    // unsigned".
    let updater = Updater::for_test(
        release.directory.path(),
        &release.executable,
        None,
        UpdateOptions {
            channel: Channel::Release,
            restart: Restart::None,
            interval_secs: None,
            automatic: false,
            manifest_url: Some(format!("{}/manifest.json", release.base)),
        },
    );

    let error = updater.check_now(None).await.unwrap_err();
    assert!(matches!(error, UpdateError::NoSigningKey), "{error}");
    assert_eq!(release.downloads(), 0);
    assert_eq!(release.on_disk(), OLD_BINARY);
}

#[tokio::test]
async fn a_manifest_for_another_channel_is_refused_even_though_it_is_signed() {
    // Validly signed by the right key, and still not the document that was
    // asked for. A signature proves who wrote it, not which channel it is.
    let release = Release::publish(Broken {
        channel: Some("beta"),
        ..Broken::default()
    })
    .await;

    let error = release.default_updater().check_now(None).await.unwrap_err();
    assert!(
        matches!(
            &error,
            UpdateError::WrongChannel {
                expected: "release",
                found
            } if found == "beta"
        ),
        "{error}"
    );
    assert_eq!(release.downloads(), 0);
}

#[tokio::test]
async fn a_release_that_needs_newer_data_is_refused_before_the_download() {
    let release = Release::publish(Broken {
        future_data_version: true,
        ..Broken::default()
    })
    .await;

    let error = release
        .default_updater()
        .apply_now(None, false)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            UpdateError::Incompatible {
                required,
                current
            } if required == DATA_VERSION + 1 && current == DATA_VERSION
        ),
        "{error}"
    );
    // The point of checking the floor first: an instance on an older database
    // learns it cannot take this release for the price of a few kilobytes.
    assert_eq!(release.downloads(), 0);
    assert_eq!(release.on_disk(), OLD_BINARY);
}

#[tokio::test]
async fn an_artifact_whose_hash_is_wrong_is_refused_and_nothing_is_written() {
    let release = Release::publish(Broken {
        wrong_hash: true,
        ..Broken::default()
    })
    .await;

    // The check passes: the manifest itself is intact and correctly signed,
    // and the bytes it lies about have not been fetched yet.
    let report = release
        .default_updater()
        .check_now(None)
        .await
        .expect("the manifest is valid");
    assert!(report.available);

    let error = release
        .default_updater()
        .apply_now(None, false)
        .await
        .unwrap_err();
    assert!(matches!(error, UpdateError::Integrity), "{error}");

    // Downloaded, then thrown away. The executable was never touched, and no
    // staged file was left behind for a later run to pick up.
    assert_eq!(release.downloads(), 1);
    assert_eq!(release.on_disk(), OLD_BINARY);
    assert!(release.previous().is_none());
    assert!(
        !release
            .directory
            .path()
            .join(".update/gproxy.staged")
            .exists()
    );
}

#[tokio::test]
async fn a_release_with_no_build_for_this_platform_says_so_on_the_check() {
    let release = Release::publish(Broken {
        wrong_target: true,
        ..Broken::default()
    })
    .await;

    let error = release.default_updater().check_now(None).await.unwrap_err();
    assert!(
        matches!(&error, UpdateError::Artifact(target) if *target == version::target()),
        "{error}"
    );
    assert_eq!(release.downloads(), 0);
}

// ------------------------------------------------------------------ apply --

#[tokio::test]
async fn a_verified_update_is_installed_backed_up_and_can_be_rolled_back() {
    let release = Release::publish(Broken::default()).await;
    let updater = release.default_updater();

    let applied = updater
        .apply_now(None, false)
        .await
        .expect("the update installs");
    assert!(applied.changed);
    assert_eq!(applied.version.as_deref(), Some("999.0.0"));
    assert_eq!(applied.restart, "none");
    assert_eq!(release.downloads(), 1);

    // The whole point: the executable's path now holds the archive's bytes,
    // and the bytes that were there are still on disk under `.prev`.
    assert_eq!(release.on_disk(), NEW_BINARY);
    assert_eq!(release.previous().as_deref(), Some(OLD_BINARY));
    // And the staged copy was cleaned up rather than left lying around as a
    // second executable.
    assert!(
        !release
            .directory
            .path()
            .join(".update/gproxy.staged")
            .exists()
    );

    // A check now sees a way back.
    let report = updater.check_now(None).await.expect("check");
    assert!(report.rollback_available);

    let rolled = updater.rollback_now(false).await.expect("rollback");
    assert!(rolled.changed);
    // Deliberately unnamed: nothing on disk records the restored binary's
    // version, and guessing would be wrong in the common case.
    assert_eq!(rolled.version, None);
    assert_eq!(release.on_disk(), OLD_BINARY);
    // Reversible in both directions.
    assert_eq!(release.previous().as_deref(), Some(NEW_BINARY));
}

#[tokio::test]
async fn a_rollback_with_nothing_to_go_back_to_is_refused() {
    let release = Release::publish(Broken::default()).await;
    let error = release
        .default_updater()
        .rollback_now(false)
        .await
        .unwrap_err();
    assert!(matches!(error, UpdateError::Rollback), "{error}");
    assert_eq!(release.on_disk(), OLD_BINARY);
}

// ---------------------------------------------------------------- restart --

/// The restart is reported before it happens, on every mode, so an operator
/// pressing apply in a console is told what is about to happen to the process.
///
/// Only `Restart::None` is *performed* here, and for the obvious reason: the
/// other two exit or replace the process image, and a test that did either
/// would take the harness with it. They are exercised against the real binary
/// instead — see the report accompanying this change.
#[tokio::test]
async fn the_restart_a_report_promises_is_the_one_configured() {
    for (mode, expected) in [
        (Restart::None, "none"),
        (Restart::Supervisor, "supervisor"),
        (Restart::ReExec, "re-exec"),
    ] {
        let release = Release::publish(Broken::default()).await;
        let updater = release.updater(UpdateOptions {
            channel: Channel::Release,
            restart: mode,
            interval_secs: None,
            automatic: false,
            manifest_url: None,
        });
        let report = updater.check_now(None).await.expect("check");
        assert_eq!(report.restart, expected, "{mode:?}");
    }
}

#[tokio::test]
async fn asking_for_a_restart_in_the_none_mode_does_nothing_at_all() {
    let release = Release::publish(Broken::default()).await;
    // `restart_after: true` is what the HTTP surface passes. With
    // `Restart::None` configured it must install and then leave the process
    // exactly where it was — this test reaching its last line is the
    // assertion.
    let applied = release
        .default_updater()
        .apply_now(None, true)
        .await
        .expect("the update installs");
    assert!(applied.changed);
    assert_eq!(applied.restart, "none");
    assert_eq!(release.on_disk(), NEW_BINARY);
}

#[tokio::test]
async fn the_app_stages_only_a_verified_tauri_apk_and_leaves_the_executable_alone() {
    let release = Release::publish(Broken {
        tauri_apk: true,
        ..Default::default()
    })
    .await;
    let apk = release
        .default_updater()
        .stage_apk()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        std::fs::read(&apk).unwrap(),
        archive(&[("gproxy", NEW_BINARY)])
    );
    assert_eq!(
        std::fs::read_to_string(apk.with_file_name("install-apk.pending")).unwrap(),
        "999.0.0"
    );
    assert_eq!(release.on_disk(), OLD_BINARY);

    for broken in [
        Broken {
            tauri_apk: true,
            wrong_key: true,
            ..Default::default()
        },
        Broken {
            tauri_apk: true,
            wrong_hash: true,
            ..Default::default()
        },
        Broken {
            tauri_apk: true,
            future_data_version: true,
            ..Default::default()
        },
        Broken::default(),
    ] {
        let release = Release::publish(broken).await;
        assert!(release.default_updater().stage_apk().await.is_err());
        assert!(
            !release
                .directory
                .path()
                .join(".update/install-apk.pending")
                .exists()
        );
        assert_eq!(release.on_disk(), OLD_BINARY);
    }
}
