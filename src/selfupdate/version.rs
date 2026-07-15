//! Update-decision logic (§19.3 / §19.4) — pure, no I/O, unit-tested.
//!
//! - `releases`: compare manifest `version` (semver) against
//!   `CARGO_PKG_VERSION`; a strictly greater manifest version is an update.
//! - `staging`: compare the manifest's commit identity against the identity
//!   embedded in the running binary; any difference is an update.

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

/// `staging` channel: build-identity compare. Official rolling builds embed the
/// commit SHA that is also carried in the signed manifest's `version` field.
/// Comparison is case-insensitive because commit SHAs are hexadecimal.
pub fn staging_decision(local_identity: &str, manifest_identity: &str) -> UpdateDecision {
    let available = !local_identity.eq_ignore_ascii_case(manifest_identity);
    UpdateDecision {
        // Short prefixes are enough to identify a staging build to a human.
        current: short_sha(local_identity),
        latest: short_sha(manifest_identity),
        available,
    }
}

fn short_sha(sha: &str) -> String {
    let n = sha.len().min(12);
    sha[..n].to_string()
}

/// The manifest target identifier of the running installation, used to pick
/// the correct release artifact. It is normally the Rust target triple. APK
/// launches add an `-apk` suffix because they must install the signed APK via
/// Android's package installer instead of replacing only the child executable.
pub fn current_target_triple() -> String {
    // env::consts gives os/arch while target_env distinguishes GNU from musl.
    // The APK launcher marks its installation kind at runtime; the very same
    // Android executable can also be distributed in a portable ZIP.
    let target_env = if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else {
        ""
    };
    target_triple_for(
        std::env::consts::ARCH,
        std::env::consts::OS,
        target_env,
        std::env::var("GPROXY_INSTALLATION_KIND").ok().as_deref(),
    )
}

pub(super) fn is_android_apk_installation() -> bool {
    cfg!(target_os = "android")
        && installation_is_android_apk(std::env::var("GPROXY_INSTALLATION_KIND").ok().as_deref())
}

fn installation_is_android_apk(installation_kind: Option<&str>) -> bool {
    installation_kind == Some("android-apk")
}

fn target_triple_for(
    arch: &str,
    os: &str,
    target_env: &str,
    installation_kind: Option<&str>,
) -> String {
    let triple = match (arch, os, target_env) {
        ("x86_64", "linux", "gnu") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux", "gnu") => "aarch64-unknown-linux-gnu",
        ("riscv64", "linux", "gnu") => "riscv64gc-unknown-linux-gnu",
        ("x86_64", "linux", "musl") => "x86_64-unknown-linux-musl",
        ("aarch64", "linux", "musl") => "aarch64-unknown-linux-musl",
        ("riscv64", "linux", "musl") => "riscv64gc-unknown-linux-musl",
        ("x86_64", "android", _) => "x86_64-linux-android",
        ("aarch64", "android", _) => "aarch64-linux-android",
        ("x86_64", "macos", _) => "x86_64-apple-darwin",
        ("aarch64", "macos", _) => "aarch64-apple-darwin",
        ("x86_64", "windows", "msvc") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows", "msvc") => "aarch64-pc-windows-msvc",
        _ => return format!("{arch}-{os}"),
    };
    if os == "android" && installation_is_android_apk(installation_kind) {
        format!("{triple}-apk")
    } else {
        triple.to_string()
    }
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
    fn staging_build_diff_is_available() {
        let d = staging_decision("aaaa1111", "bbbb2222");
        assert!(d.available);
    }

    #[test]
    fn staging_build_same_not_available() {
        let d = staging_decision("deadbeefcafef00d", "DEADBEEFCAFEF00D");
        assert!(!d.available, "same commit identity → no update");
        assert_eq!(d.current, "deadbeefcafe");
    }

    #[test]
    fn maps_riscv64_linux_release_target() {
        assert_eq!(
            target_triple_for("riscv64", "linux", "gnu", None),
            "riscv64gc-unknown-linux-gnu"
        );
    }

    #[test]
    fn distinguishes_all_linux_musl_release_targets() {
        assert_eq!(
            target_triple_for("x86_64", "linux", "musl", None),
            "x86_64-unknown-linux-musl"
        );
        assert_eq!(
            target_triple_for("aarch64", "linux", "musl", None),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            target_triple_for("riscv64", "linux", "musl", None),
            "riscv64gc-unknown-linux-musl"
        );
    }

    #[test]
    fn distinguishes_portable_android_from_apk_installations() {
        assert_eq!(
            target_triple_for("aarch64", "android", "", None),
            "aarch64-linux-android"
        );
        assert_eq!(
            target_triple_for("aarch64", "android", "", Some("android-apk")),
            "aarch64-linux-android-apk"
        );
        assert_eq!(
            target_triple_for("x86_64", "android", "", Some("android-apk")),
            "x86_64-linux-android-apk"
        );
    }
}
