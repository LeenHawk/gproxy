//! Anthropic JS SDK runtime fingerprint used by Claude Code 2.1.258.

use http::HeaderMap;
use http::header::{HeaderName, HeaderValue};

use crate::channel::ChannelError;

const STATIC_HEADERS: &[(&str, &str)] = &[
    ("x-stainless-retry-count", "0"),
    ("x-stainless-timeout", "86400"),
    ("x-stainless-lang", "js"),
    ("x-stainless-package-version", "0.112.1"),
    ("x-stainless-runtime", "node"),
    ("x-stainless-runtime-version", "v26.3.0"),
];

pub(super) fn apply(headers: &mut HeaderMap) -> Result<(), ChannelError> {
    for (name, value) in STATIC_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    let (target_os, target_arch) = platform();
    insert(headers, "x-stainless-os", &os(target_os))?;
    insert(headers, "x-stainless-arch", &arch(target_arch))
}

#[cfg(not(target_arch = "wasm32"))]
fn platform() -> (&'static str, &'static str) {
    (std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(target_arch = "wasm32")]
fn platform() -> (&'static str, &'static str) {
    // Edge cannot expose its host CPU; retain the captured Node reference pair.
    ("linux", "x86_64")
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), ChannelError> {
    let value = HeaderValue::from_str(value)
        .map_err(|e| ChannelError::Build(format!("bad {name}: {e}")))?;
    headers.insert(HeaderName::from_static(name), value);
    Ok(())
}

pub(super) fn os(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "ios" => "iOS".into(),
        "android" => "Android".into(),
        "darwin" | "macos" => "MacOS".into(),
        "win32" | "windows" => "Windows".into(),
        "freebsd" => "FreeBSD".into(),
        "openbsd" => "OpenBSD".into(),
        "linux" => "Linux".into(),
        "" | "unknown" => "Unknown".into(),
        other => format!("Other:{other}"),
    }
}

pub(super) fn arch(value: &str) -> String {
    match value {
        "x32" => "x32".into(),
        "x86_64" | "x64" => "x64".into(),
        "arm" => "arm".into(),
        "aarch64" | "arm64" => "arm64".into(),
        "" | "unknown" => "unknown".into(),
        other => format!("other:{other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rust_targets_to_stainless_values() {
        assert_eq!(os("linux"), "Linux");
        assert_eq!(os("macos"), "MacOS");
        assert_eq!(os("windows"), "Windows");
        assert_eq!(arch("x86_64"), "x64");
        assert_eq!(arch("aarch64"), "arm64");
        assert_eq!(arch("riscv64"), "other:riscv64");
    }
}
