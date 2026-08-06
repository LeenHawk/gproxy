//! Build identity of the running binary (§19.3) — what `--version` reports.
//!
//! Release artifacts are UPX-packed, so `file` and `ldd` describe *every* Linux
//! build as "statically linked" / "not a dynamic executable": the GNU and musl
//! artifacts are indistinguishable from the outside, and users cannot tell
//! which one they are running. The target triple is also exactly the key
//! self-update uses to pick its artifact ([`super::current_target_triple`]), so
//! printing it makes "did the update give me the wrong libc?" answerable
//! without unpacking the binary.

/// Commit identity embedded by the release workflow (`GPROXY_BUILD_VERSION`):
/// the tag for tagged builds, the commit SHA for rolling `main` builds. Absent
/// in local/custom builds.
pub(super) fn build_identity() -> Option<&'static str> {
    normalized_build_identity(option_env!("GPROXY_BUILD_VERSION"))
}

/// Normalization split out so it can be tested without the compile-time
/// `GPROXY_BUILD_VERSION`.
fn normalized_build_identity(identity: Option<&str>) -> Option<&str> {
    identity.map(str::trim).filter(|value| !value.is_empty())
}

/// Long `--version` text: package version, self-update target triple, update
/// channel, and — for official builds — the embedded commit identity.
///
/// ```text
/// gproxy 2.3.1 (x86_64-unknown-linux-musl, channel staging, build 589569af7f72)
/// ```
///
/// Computed once and kept for the process lifetime because clap's `version`
/// builder takes a `&'static str`.
pub fn version_line() -> &'static str {
    static LINE: std::sync::LazyLock<String> = std::sync::LazyLock::new(build_version_line);
    LINE.as_str()
}

fn build_version_line() -> String {
    let mut details = vec![
        super::current_target_triple(),
        format!("channel {}", super::build_channel().as_str()),
    ];
    if let Some(identity) = build_identity() {
        details.push(format!("build {}", super::short_identity(identity)));
    }
    format!("{} ({})", env!("CARGO_PKG_VERSION"), details.join(", "))
}

#[cfg(test)]
mod tests {
    use super::{normalized_build_identity, version_line};

    #[test]
    fn build_identity_is_trimmed_and_must_not_be_empty() {
        assert_eq!(
            normalized_build_identity(Some("  deadbeef  ")),
            Some("deadbeef")
        );
        assert_eq!(normalized_build_identity(Some("  ")), None);
        assert_eq!(normalized_build_identity(None), None);
    }

    #[test]
    fn version_line_reports_package_version_target_and_channel() {
        let line = version_line();
        assert!(line.starts_with(env!("CARGO_PKG_VERSION")));
        assert!(line.contains(&crate::selfupdate::current_target_triple()));
        assert!(line.contains("channel "));
    }
}
