//! Update-decision logic (§19.3 / §19.4) — pure, no I/O, unit-tested.
//!
//! - `releases`: compare manifest `version` (semver) against
//!   `CARGO_PKG_VERSION`; a strictly greater manifest version is an update.
//! - `staging`: `version` is meaningless, so compare the manifest artifact's
//!   sha256 against the running binary's sha256; any difference is an update.

use semver::Version;

use super::UpdateError;

/// Outcome of a channel decision: the human-facing current/latest identities
/// and whether an update should be offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateDecision {
    pub current: String,
    pub latest: String,
    pub available: bool,
}

/// `releases` channel: semver compare manifest `version` vs the compiled-in
/// `CARGO_PKG_VERSION`. Available iff the manifest version is strictly greater.
pub fn releases_decision(manifest_version: &str) -> Result<UpdateDecision, UpdateError> {
    let current_str = env!("CARGO_PKG_VERSION");
    decide_semver(current_str, manifest_version)
}

/// Semver comparison split out so it can be tested without the compile-time
/// `CARGO_PKG_VERSION`.
fn decide_semver(current_str: &str, manifest_version: &str) -> Result<UpdateDecision, UpdateError> {
    let current = Version::parse(current_str)
        .map_err(|e| UpdateError::Version(format!("{current_str}: {e}")))?;
    // Manifest tags may carry a leading `v` (e.g. `v2.1.0`); strip it.
    let latest_trimmed = manifest_version
        .strip_prefix('v')
        .unwrap_or(manifest_version);
    let latest = Version::parse(latest_trimmed).map_err(|e| {
        UpdateError::Manifest(format!("bad manifest version `{manifest_version}`: {e}"))
    })?;

    Ok(UpdateDecision {
        current: current.to_string(),
        latest: latest.to_string(),
        available: latest > current,
    })
}

/// `staging` channel: sha256 compare. Available iff the manifest artifact's
/// sha256 differs from the running binary's. Comparison is case-insensitive on
/// the hex (defensive — both should be lowercase hex).
pub fn staging_decision(local_sha256: &str, manifest_sha256: &str) -> UpdateDecision {
    let available = !local_sha256.eq_ignore_ascii_case(manifest_sha256);
    UpdateDecision {
        // Short prefixes are enough to identify a staging build to a human.
        current: short_sha(local_sha256),
        latest: short_sha(manifest_sha256),
        available,
    }
}

fn short_sha(sha: &str) -> String {
    let n = sha.len().min(12);
    sha[..n].to_string()
}

/// The target triple of the running binary, used to pick the manifest artifact.
/// Built from compile-time `cfg` so it always matches the binary in hand.
pub fn current_target_triple() -> String {
    // env::consts gives os/arch; map to the conventional Rust triple. This
    // covers the platforms GPROXY ships; an unmatched combo falls back to a
    // best-effort `arch-os` string that simply won't match any artifact (→
    // NoArtifact, which is the correct, safe outcome).
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => return format!("{arch}-{os}"),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_newer_is_available() {
        let d = decide_semver("2.0.0", "2.1.0").unwrap();
        assert!(d.available);
        assert_eq!(d.current, "2.0.0");
        assert_eq!(d.latest, "2.1.0");
    }

    #[test]
    fn semver_same_or_older_not_available() {
        assert!(!decide_semver("2.1.0", "2.1.0").unwrap().available);
        assert!(!decide_semver("2.1.0", "2.0.9").unwrap().available);
        assert!(!decide_semver("2.1.0", "1.9.9").unwrap().available);
    }

    #[test]
    fn semver_strips_leading_v() {
        let d = decide_semver("2.0.0", "v2.0.1").unwrap();
        assert!(d.available);
        assert_eq!(d.latest, "2.0.1");
    }

    #[test]
    fn semver_prerelease_ordering() {
        // 2.1.0 > 2.1.0-rc.1 per semver.
        assert!(!decide_semver("2.1.0", "2.1.0-rc.1").unwrap().available);
        assert!(decide_semver("2.1.0-rc.1", "2.1.0").unwrap().available);
    }

    #[test]
    fn semver_bad_manifest_version_errors() {
        assert!(matches!(
            decide_semver("2.0.0", "not-a-version"),
            Err(UpdateError::Manifest(_))
        ));
    }

    #[test]
    fn staging_sha_diff_is_available() {
        let d = staging_decision("aaaa1111", "bbbb2222");
        assert!(d.available);
    }

    #[test]
    fn staging_sha_same_not_available() {
        let d = staging_decision("deadbeefcafef00d", "DEADBEEFCAFEF00D");
        assert!(!d.available, "case-insensitive equal shas → no update");
        assert_eq!(d.current, "deadbeefcafe");
    }
}
