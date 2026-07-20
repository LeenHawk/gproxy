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
//! - `staging`: one fixed `staging` tag, CI re-uploads in place; update is
//!   decided by comparing the signed manifest's commit identity to the commit
//!   identity embedded in the running binary.
//!
//! Trust anchor (§19.2): when the manifest carries sha256/signature metadata,
//! the binary is verified before replacement. Missing safety metadata is never
//! silently ignored: HTTP callers must ask for explicit operator confirmation
//! before applying such an update. The risky I/O (download, integrity/signature
//! check, atomic swap, restart) lives behind the [`download`], [`verify`], and
//! [`swap`] seams; [`version`] and [`manifest`] are pure and unit-tested.

#[cfg(not(target_arch = "wasm32"))]
mod android_apk;
#[cfg(not(target_arch = "wasm32"))]
mod applied;
#[cfg(not(target_arch = "wasm32"))]
mod apply;
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
pub use apply::{apply, apply_with_options};
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
    /// Fixed `staging` tag, rolling re-upload; commit-identity comparison.
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

/// Update channel embedded by the release workflow. Tagged builds track stable
/// releases; rolling builds from `main` track staging. Local/custom builds
/// default to stable unless the operator overrides the serve-path CLI/env.
pub fn build_channel() -> Channel {
    channel_from_build_label(option_env!("GPROXY_BUILD_CHANNEL"))
}

fn channel_from_build_label(label: Option<&str>) -> Channel {
    match label {
        Some("staging") => Channel::Staging,
        _ => Channel::Releases,
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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateSafetyRisk {
    MissingSha256,
    MissingSignature,
    MissingPublicKey,
}

/// How the selected artifact is installed after verification.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallMode {
    Binary,
    AndroidApk,
}

#[cfg(not(target_arch = "wasm32"))]
fn current_install_mode() -> UpdateInstallMode {
    if version::is_android_apk_installation() {
        UpdateInstallMode::AndroidApk
    } else {
        UpdateInstallMode::Binary
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions {
    pub restart: Restart,
    pub allow_insecure: bool,
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
    #[error("update requires confirmation: {0}")]
    ConfirmationRequired(String),
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
    /// Safety metadata that is absent from the manifest or this binary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safety: Vec<UpdateSafetyRisk>,
    /// Whether apply replaces the executable or hands a verified APK to the
    /// Android system package installer.
    pub install_mode: UpdateInstallMode,
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
                safety: Vec::new(),
                install_mode: current_install_mode(),
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
            let local = staging_current_identity()?;
            version::staging_decision(&local, &manifest.version)
        }
    };
    let safety = if decision.available {
        safety_risks(&manifest, artifact)
    } else {
        Vec::new()
    };

    Ok(CheckReport {
        current: decision.current,
        latest: decision.latest,
        available: decision.available,
        notes_url: manifest.notes_url.clone(),
        safety,
        install_mode: current_install_mode(),
    })
}

/// The current identity string for a channel (semver for `releases`, sha256
/// short prefix for `staging`) — used to fill a [`CheckReport`] when there is no
/// manifest to compare against.
#[cfg(not(target_arch = "wasm32"))]
fn current_identity(ctx: &UpdateContext) -> Result<String, UpdateError> {
    Ok(match ctx.channel {
        Channel::Releases => env!("CARGO_PKG_VERSION").to_string(),
        Channel::Staging => short_identity(&staging_current_identity()?),
    })
}

/// Identity of the running staging build. Official CI builds embed the commit
/// SHA, matching the signed manifest `version`. Legacy/custom binaries have no
/// embedded identity; falling back to their executable hash deliberately makes
/// them take one update into the new identity scheme.
#[cfg(not(target_arch = "wasm32"))]
fn staging_current_identity() -> Result<String, UpdateError> {
    if let Some(identity) = normalized_build_identity(option_env!("GPROXY_BUILD_VERSION")) {
        return Ok(identity.to_string());
    }
    swap::current_exe_sha256()
}

fn normalized_build_identity(identity: Option<&str>) -> Option<&str> {
    identity.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(not(target_arch = "wasm32"))]
fn safety_risks(manifest: &Manifest, artifact: &Artifact) -> Vec<UpdateSafetyRisk> {
    let mut risks = Vec::new();
    if artifact.sha256_value().is_none() {
        risks.push(UpdateSafetyRisk::MissingSha256);
    }
    if manifest.signature_value().is_none() {
        risks.push(UpdateSafetyRisk::MissingSignature);
    } else if !verify::has_embedded_pubkey() {
        risks.push(UpdateSafetyRisk::MissingPublicKey);
    }
    risks
}

#[cfg(not(target_arch = "wasm32"))]
fn safety_message(risks: &[UpdateSafetyRisk]) -> String {
    let labels = risks
        .iter()
        .map(|risk| match risk {
            UpdateSafetyRisk::MissingSha256 => "artifact sha256",
            UpdateSafetyRisk::MissingSignature => "manifest signature",
            UpdateSafetyRisk::MissingPublicKey => "embedded update public key",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("missing safety metadata: {labels}")
}

#[cfg(not(target_arch = "wasm32"))]
fn short_identity(value: &str) -> String {
    value.chars().take(12).collect()
}

/// Restart the running process after a previously staged update.
///
/// This is intentionally separate from [`apply`] so HTTP callers can send a
/// terminal response before the process is replaced.
#[cfg(not(target_arch = "wasm32"))]
pub fn restart(restart: Restart) -> ! {
    restart_now(restart)
}

#[cfg(not(target_arch = "wasm32"))]
fn restart_now(restart: Restart) -> ! {
    match restart {
        Restart::Supervisor => swap::exit_for_supervisor(),
        // An APK cannot replace itself. Exit back to the Java foreground
        // service, which sees the sentinel and launches the system installer.
        Restart::ReExec if version::is_android_apk_installation() => swap::exit_for_supervisor(),
        Restart::ReExec => swap::reexec(),
        Restart::None => std::process::exit(0),
    }
}

#[cfg(test)]
mod build_channel_tests {
    use super::{Channel, channel_from_build_label, normalized_build_identity};

    #[test]
    fn build_label_selects_expected_channel() {
        assert_eq!(channel_from_build_label(Some("staging")), Channel::Staging);
        assert_eq!(
            channel_from_build_label(Some("releases")),
            Channel::Releases
        );
        assert_eq!(channel_from_build_label(None), Channel::Releases);
    }

    #[test]
    fn build_identity_is_trimmed_and_must_not_be_empty() {
        assert_eq!(
            normalized_build_identity(Some("  deadbeef  ")),
            Some("deadbeef")
        );
        assert_eq!(normalized_build_identity(Some("  ")), None);
        assert_eq!(normalized_build_identity(None), None);
    }
}
