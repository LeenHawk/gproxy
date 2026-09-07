use semver::Version;

use super::{Error, Result};

pub(super) fn available(channel: &str, latest: &str) -> Result<(String, bool)> {
    if channel == "staging" {
        let current = crate::BUILD_HASH.to_owned();
        return Ok((current.clone(), !current.eq_ignore_ascii_case(latest)));
    }
    let current_text = crate::BUILD_VERSION.trim_start_matches('v');
    let current = Version::parse(current_text).map_err(|_| Error::Version)?;
    let latest = Version::parse(latest.trim_start_matches('v')).map_err(|_| Error::Version)?;
    Ok((current.to_string(), latest > current))
}

pub(super) fn compatible(required: u32, current: u32) -> Result<()> {
    if required > current {
        Err(Error::Incompatible)
    } else {
        Ok(())
    }
}

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
    let base = match (arch, os, environment) {
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
    // The APK and the Termux archive come out of one Android build, so the
    // artifact to update with follows the running process, not the build.
    #[cfg(target_os = "android")]
    {
        if crate::android::runtime().is_app() {
            return format!("{base}-apk");
        }
    }
    base.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dev_channel_uses_versioned_alpha_ordering() {
        assert_eq!(
            super::available("dev", "999.0.0-alpha.1").unwrap(),
            super::available("releases", "999.0.0-alpha.1").unwrap()
        );
    }

    #[test]
    fn compatibility_gate_refuses_a_newer_required_data_floor() {
        assert!(super::compatible(10, 9).is_err());
        assert!(super::compatible(9, 9).is_ok());
    }
}
