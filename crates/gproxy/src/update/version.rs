//! Which artifact this process is, whether the manifest offers a newer one,
//! and whether the release can open this instance's data.
//!
//! # The data-version gate, and what it compares against in v4
//!
//! v3 numbered its migrations, so the manifest's `min_compatible_data_version`
//! had an obvious counterpart: `SchemaVersion::LATEST.number()`. v4's store
//! has no such number — `Store::sync` reconciles the schema against the entity
//! definitions instead, so there is no ordered list to take a maximum of.
//!
//! So the counterpart here is [`DATA_VERSION`]: a hand-maintained generation
//! of the *data layout*, bumped when a release stops being able to open a
//! database an older one wrote. The gate is unchanged in meaning — a release
//! that declares a floor above this number is refused before anything is
//! downloaded — but the number is now a deliberate statement rather than a
//! derived one, and that has a consequence worth writing down:
//! `scripts/build-update-manifest.sh` still derives its `minimum` from
//! `crates/gproxy-store/src/schema/catalog.rs`, **a file v4 does not have**.
//! Whoever revives the release pipeline has to point it at this constant.

use semver::Version;

use super::config::{BUILD_HASH, BUILD_VERSION, Channel, UpdateError};

/// The generation of the on-disk data layout this binary understands.
///
/// Compared against a manifest's `min_compatible_data_version`. Bump it in the
/// same commit as a store change that an older release cannot read, and the
/// older release will then refuse to install this one rather than starting
/// against a database it cannot open.
pub const DATA_VERSION: u32 = 4;

/// The current version and whether `latest` is newer.
///
/// `dev` compares commit hashes: a rolling build of the default branch has no
/// version to order, so "different" is the only available meaning of "newer".
/// `beta` and `release` are semver, which is what makes `4.1.0-rc.1` correctly
/// older than `4.1.0` on the `beta` channel.
pub(super) fn available(channel: Channel, latest: &str) -> Result<(String, bool), UpdateError> {
    if channel == Channel::Dev {
        let current = BUILD_HASH.to_owned();
        let newer = !current.eq_ignore_ascii_case(latest);
        return Ok((current, newer));
    }
    let current = parse(BUILD_VERSION)?;
    let latest = parse(latest)?;
    Ok((current.to_string(), latest > current))
}

fn parse(value: &str) -> Result<Version, UpdateError> {
    Version::parse(value.trim().trim_start_matches('v'))
        .map_err(|_| UpdateError::Version(value.to_owned()))
}

/// Refuse a release that needs a newer data layout than this build has.
///
/// Checked **before** the artifact is downloaded, so an instance on an older
/// database spends nothing finding out.
pub(super) fn compatible(required: u32, current: u32) -> Result<(), UpdateError> {
    if required > current {
        Err(UpdateError::Incompatible { required, current })
    } else {
        Ok(())
    }
}

/// The artifact key this process should look itself up by.
///
/// Composed from `std::env::consts` and the target environment rather than
/// from a build-time `TARGET`, so it is right for a binary built by anyone —
/// including `cargo install`, which sets no such variable. The fallback
/// `{arch}-{os}` is deliberately something no manifest will contain: an
/// unrecognised platform gets "this release has no artifact for …", which is
/// true, rather than being offered somebody else's binary.
pub(super) fn target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let environment = if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else {
        ""
    };
    match (arch, os, environment) {
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
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_version_is_available_and_an_older_one_is_not() {
        // `BUILD_VERSION` in a checkout is the crate version, so the
        // comparison is anchored on something that certainly parses rather
        // than on a literal.
        let (current, newer) = available(Channel::Release, "999.0.0").unwrap();
        assert!(newer, "999.0.0 should be newer than {current}");
        assert!(!available(Channel::Release, "0.0.1").unwrap().1);
        assert!(
            !available(Channel::Release, BUILD_VERSION).unwrap().1,
            "the running version is not an update"
        );
    }

    #[test]
    fn a_leading_v_is_not_part_of_the_version() {
        assert_eq!(
            available(Channel::Release, "999.0.0").unwrap(),
            available(Channel::Release, "v999.0.0").unwrap()
        );
    }

    #[test]
    fn a_prerelease_orders_below_its_release_on_the_beta_channel() {
        assert!(available(Channel::Beta, "999.0.0-rc.1").unwrap().1);
        assert!(available(Channel::Beta, "999.0.0").unwrap().1);
        assert!(
            Version::parse("999.0.0-rc.1").unwrap() < Version::parse("999.0.0").unwrap(),
            "semver ordering is what the beta channel relies on"
        );
    }

    #[test]
    fn a_version_that_does_not_parse_names_itself() {
        let error = available(Channel::Release, "the-good-one").unwrap_err();
        assert!(error.to_string().contains("the-good-one"), "{error}");
    }

    /// A rolling branch has no versions, so the question is only "is this a
    /// different commit".
    #[test]
    fn dev_compares_commits_rather_than_versions() {
        let (current, newer) = available(Channel::Dev, "deadbeef").unwrap();
        assert_eq!(current, BUILD_HASH);
        assert!(newer);
        assert!(!available(Channel::Dev, BUILD_HASH).unwrap().1);
        // And a version string is not special on this channel: it is simply a
        // hash that does not match.
        assert!(available(Channel::Dev, "4.1.0").unwrap().1);
    }

    #[test]
    fn compatibility_gate_refuses_a_newer_required_data_floor() {
        assert!(compatible(DATA_VERSION, DATA_VERSION).is_ok());
        assert!(compatible(DATA_VERSION - 1, DATA_VERSION).is_ok());
        let error = compatible(DATA_VERSION + 1, DATA_VERSION).unwrap_err();
        assert!(error.to_string().contains("data version"), "{error}");
    }

    #[test]
    fn the_target_is_a_triple_a_release_actually_publishes() {
        let target = target();
        assert!(!target.is_empty());
        // Whatever this test runs on, the answer must contain the running
        // architecture, or the lookup would silently match the wrong build.
        assert!(
            target.starts_with(std::env::consts::ARCH) || target.starts_with("riscv64gc"),
            "{target}"
        );
    }
}
