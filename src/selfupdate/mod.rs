//! Self-update mechanism (§19) — NATIVE only.
//!
//! The `gproxy` binary embeds the Console (rust-embed) and carries no business
//! data (config/credentials live in the persistence layer), so self-update only
//! swaps the executable. Edge (wasm) builds deploy through platform pipelines
//! and do NOT self-update: every item here is `#[cfg(not(target_arch =
//! "wasm32"))]`.
//!
//! Two orthogonal release channels (§19.3):
//! - `releases`: each version is a `vX.X.X` tag/Release; update decided by
//!   **semver** (manifest `version` vs `CARGO_PKG_VERSION`).
//! - `staging`: one fixed `staging` tag, CI re-uploads in place; `version` is
//!   meaningless, so update is decided by comparing the manifest artifact
//!   **sha256** to the running binary's sha256.
//!
//! Trust anchor (§19.2): the manifest is ed25519-signed; the public key is
//! compiled in. No valid signature → the binary is never replaced. The risky
//! I/O (download, integrity/signature check, atomic swap, restart) lives behind
//! the [`download`], [`verify`], and [`swap`] seams; [`version`] and
//! [`manifest`] are pure and unit-tested.

#[cfg(not(target_arch = "wasm32"))]
mod applied;
#[cfg(not(target_arch = "wasm32"))]
mod download;
#[cfg(not(target_arch = "wasm32"))]
mod extract;
#[cfg(not(target_arch = "wasm32"))]
mod manifest;
#[cfg(not(target_arch = "wasm32"))]
mod swap;
#[cfg(not(target_arch = "wasm32"))]
mod verify;
#[cfg(not(target_arch = "wasm32"))]
mod version;

#[cfg(not(target_arch = "wasm32"))]
pub use manifest::{Artifact, Manifest};
#[cfg(not(target_arch = "wasm32"))]
pub use version::{UpdateDecision, current_target_triple};

/// Built-in GitHub repository used by native self-update.
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_REPO: &str = "LeenHawk/gproxy";

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::http::client::UpstreamClient;

/// Release channel (§19.3). One of the two `update_channel` values.
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[cfg_attr(not(target_arch = "wasm32"), value(rename_all = "lowercase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Channel {
    /// Versioned `vX.X.X` releases; semver comparison. Production default.
    #[default]
    Releases,
    /// Fixed `staging` tag, rolling re-upload; sha256 comparison.
    Staging,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Channel::Releases => "releases",
            Channel::Staging => "staging",
        }
    }
}

/// Update policy (§19.4). Governs whether a detected update is applied.
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[cfg_attr(not(target_arch = "wasm32"), value(rename_all = "lowercase"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// Never check.
    Off,
    /// Report availability only.
    Notify,
    /// Check + report; admin approval applies. Default.
    #[default]
    Manual,
    /// Check + apply + restart (opt-in, risky).
    Auto,
}

/// Restart model after a successful swap (§19.6).
#[cfg_attr(not(target_arch = "wasm32"), derive(clap::ValueEnum))]
#[cfg_attr(not(target_arch = "wasm32"), value(rename_all = "kebab-case"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Restart {
    /// Exit with a sentinel code; the supervisor (systemd/docker/k8s) restarts
    /// the new binary. Default for container deploys.
    #[default]
    Supervisor,
    /// `execv` the new binary in place (bare deploy, no supervisor).
    ReExec,
    /// Stage only; do not restart (the caller decides).
    None,
}

/// Errors surfaced by the self-update flow.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("manifest fetch/parse failed: {0}")]
    Manifest(String),
    #[error("no update manifest published for this channel")]
    ManifestNotFound,
    #[error("no artifact in manifest for target `{0}`")]
    NoArtifact(String),
    #[error("download failed: {0}")]
    Download(String),
    #[error("integrity check failed: {0}")]
    Integrity(String),
    #[error("signature verification failed: {0}")]
    Signature(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("binary swap failed: {0}")]
    Swap(String),
    #[error("current version is not valid semver: {0}")]
    Version(String),
    #[error("update refused — incompatible data version: {0}")]
    Incompatible(String),
    #[error("update refused — downgrade/rollback blocked: {0}")]
    Downgrade(String),
}

/// The data/schema version this binary operates at — the floor the manifest's
/// `min_compatible_data_version` is checked against (§19.7). Sourced from the
/// migration list (the running binary migrated the store to this version on
/// boot). `0` in a no-persistence build, so the check simply never fires there.
#[cfg(not(target_arch = "wasm32"))]
fn current_data_version() -> u32 {
    #[cfg(any(feature = "persist-db", feature = "persist-file"))]
    let v = crate::store::persistence::migrations::latest_version().max(0) as u32;
    #[cfg(not(any(feature = "persist-db", feature = "persist-file")))]
    let v = 0u32;
    v
}

/// Result of a `check` (§19.10 `GET /admin/update/check` shape).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckReport {
    /// Current identity (semver for `releases`, sha256 prefix for `staging`).
    pub current: String,
    /// Latest identity from the manifest.
    pub latest: String,
    /// Whether an update is available.
    pub available: bool,
    /// Release notes URL, if the manifest carries one.
    pub notes_url: Option<String>,
}

/// Runtime context for a self-update run.
#[cfg(not(target_arch = "wasm32"))]
pub struct UpdateContext {
    /// GitHub `owner/repo` whose Releases host the manifest + artifacts.
    pub repo: String,
    /// Channel to track.
    pub channel: Channel,
    /// Data directory; staging happens under `<data_dir>/.update`.
    pub data_dir: PathBuf,
    /// Proxy-aware HTTP transport (reuses the upstream client).
    pub client: Arc<dyn UpstreamClient>,
}

/// Check the configured channel for an available update (§19.4). Pure decision
/// logic lives in [`version`]; this only does the manifest fetch + dispatch.
///
/// A channel with **no published manifest** is reported as "no update available"
/// (not an error): a missing manifest must never surface to the operator as a
/// 500 (§19.10).
#[cfg(not(target_arch = "wasm32"))]
pub async fn check(ctx: &UpdateContext) -> Result<CheckReport, UpdateError> {
    let manifest = match download::fetch_manifest(ctx).await {
        Ok(m) => m,
        Err(UpdateError::ManifestNotFound) => {
            let current = current_identity(ctx)?;
            tracing::info!(
                channel = ctx.channel.as_str(),
                "no update manifest published; reporting up-to-date"
            );
            return Ok(CheckReport {
                latest: current.clone(),
                current,
                available: false,
                notes_url: None,
            });
        }
        Err(e) => return Err(e),
    };
    let triple = current_target_triple();
    let artifact = manifest
        .artifact_for(&triple)
        .ok_or_else(|| UpdateError::NoArtifact(triple.clone()))?;

    let decision = match ctx.channel {
        Channel::Releases => version::releases_decision(&manifest.version)?,
        Channel::Staging => {
            let local = swap::current_exe_sha256()?;
            version::staging_decision(&local, &artifact.sha256)
        }
    };

    Ok(CheckReport {
        current: decision.current,
        latest: decision.latest,
        available: decision.available,
        notes_url: manifest.notes_url.clone(),
    })
}

/// The current identity string for a channel (semver for `releases`, sha256
/// short prefix for `staging`) — used to fill a [`CheckReport`] when there is no
/// manifest to compare against.
#[cfg(not(target_arch = "wasm32"))]
fn current_identity(ctx: &UpdateContext) -> Result<String, UpdateError> {
    Ok(match ctx.channel {
        Channel::Releases => env!("CARGO_PKG_VERSION").to_string(),
        Channel::Staging => {
            let sha = swap::current_exe_sha256()?;
            sha[..sha.len().min(12)].to_string()
        }
    })
}

/// Download, verify (sha256 + ed25519 signature), atomically swap, and (per
/// `restart`) hand off to a new binary (§19.5 / §19.6). Returns the new
/// version/identity on success when no restart is requested.
///
/// `ReExec` does not return on success (it replaces the process image);
/// `Supervisor` exits the process with the sentinel code after staging.
#[cfg(not(target_arch = "wasm32"))]
pub async fn apply(ctx: &UpdateContext, restart: Restart) -> Result<String, UpdateError> {
    let manifest = download::fetch_manifest(ctx).await?;
    let triple = current_target_triple();
    let artifact = manifest
        .artifact_for(&triple)
        .ok_or_else(|| UpdateError::NoArtifact(triple.clone()))?
        .clone();

    // §19.7 data-compat floor (any channel): refuse a binary that requires a
    // newer on-disk data schema than this deployment has — it would boot against
    // data it can't read. The field is signed into the manifest, so this gate is
    // as trustworthy as the signature.
    let required = manifest.min_compatible_data_version;
    let have = current_data_version();
    if required > have {
        return Err(UpdateError::Incompatible(format!(
            "manifest needs data version >= {required}, but this store is at {have}; \
             migrate data before updating"
        )));
    }

    // The running binary's sha256 — for staging it drives both the
    // already-up-to-date gate and the rollback guard, so compute it once.
    let local_sha = match ctx.channel {
        Channel::Staging => Some(swap::current_exe_sha256()?),
        Channel::Releases => None,
    };

    // Gate: only proceed if there is actually something to install.
    let available = match &local_sha {
        Some(local) => version::staging_decision(local, &artifact.sha256).available,
        None => version::releases_decision(&manifest.version)?.available,
    };
    if !available {
        tracing::info!(channel = ctx.channel.as_str(), "already up to date");
        return Ok(manifest.version.clone());
    }

    // Staging rollback guard (§19.3): `staging` decides by sha and has no version
    // ordering, so a replayed older-but-validly-signed manifest could roll the
    // binary backward. Refuse a sha we've already superseded. `releases` is
    // ordered by semver vs the compiled-in version and needs no ledger.
    if let Some(local) = &local_sha
        && applied::is_rollback(&applied::load(&ctx.data_dir), local, &artifact.sha256)
    {
        return Err(UpdateError::Downgrade(format!(
            "staging artifact {} was already superseded by a newer build",
            applied::short(&artifact.sha256)
        )));
    }

    // 1. Download the artifact (a release `.zip`) to a temp file on the same
    //    filesystem as the binary.
    let staged_zip = download::download_artifact(ctx, &artifact).await?;

    // 2. Integrity: sha256 of the downloaded ZIP must equal the manifest's.
    verify::verify_sha256(&staged_zip, &artifact.sha256)?;

    // 3. Signature: the embedded ed25519 public key must verify the manifest
    //    signature (§19.2 — the hard floor; staging is verified too).
    verify::verify_manifest_signature(&manifest)?;

    // 4. Extract the `gproxy` executable from the verified zip. The zip's bytes
    //    were sha256-checked and that sha is bound by the manifest signature, so
    //    the extracted binary inherits that trust — no separate inner-hash needed.
    let staged_bin = extract::extract_binary(&staged_zip)?;

    // 5. Atomic swap, retaining `<exe>.prev` for rollback (§19.5 / §19.8).
    swap::install(&staged_bin)?;
    // Record the applied sha so a later replay of this (now-superseded) build is
    // caught by the rollback guard above. Staging only; best-effort.
    if let Some(local) = &local_sha {
        applied::record(&ctx.data_dir, local, &artifact.sha256);
    }
    tracing::info!(
        channel = ctx.channel.as_str(),
        version = %manifest.version,
        "new binary staged and verified"
    );

    // 6. Restart / hand off (§19.6).
    match restart {
        Restart::Supervisor => swap::exit_for_supervisor(),
        Restart::ReExec => swap::reexec(), // diverges on success
        Restart::None => Ok(manifest.version.clone()),
    }
}

/// Restart the running process after a previously staged update.
///
/// This is intentionally separate from [`apply`] so HTTP callers can send a
/// terminal response before the process is replaced.
#[cfg(not(target_arch = "wasm32"))]
pub fn restart(restart: Restart) -> ! {
    match restart {
        Restart::Supervisor => swap::exit_for_supervisor(),
        Restart::ReExec => swap::reexec(),
        Restart::None => std::process::exit(0),
    }
}
