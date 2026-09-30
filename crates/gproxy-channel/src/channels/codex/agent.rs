//! The Codex CLI's `User-Agent`, `codex_cli_rs/<version> (<os> <version>;
//! <arch>) <terminal>` (`login/src/auth/default_client.rs::get_codex_user_agent`
//! with `os_info` and `codex-terminal-detection`). The host part is computed
//! once from the same sources `os_info` reads without spawning anything:
//! `/etc/os-release` on Linux, `SystemVersion.plist` on macOS; Windows and
//! wasm report an unknown version. The terminal token follows the CLI's
//! fallback chain (`TERM_PROGRAM`, then `TERM`, else `unknown`).

use std::sync::OnceLock;

/// The release this channel impersonates (openai/codex `rust-v0.159.2`;
/// the mirrored checkout carries the placeholder `0.0.0`).
pub const CLI_VERSION: &str = "0.159.2";

pub(super) fn user_agent(originator: &str) -> String {
    static HOST: OnceLock<String> = OnceLock::new();
    let host = HOST.get_or_init(|| {
        let (os, version) = os_name_and_version();
        format!("({os} {version}; {}) {}", architecture(), terminal())
    });
    format!("{originator}/{CLI_VERSION} {host}")
}

#[cfg(target_arch = "wasm32")]
fn os_name_and_version() -> (String, String) {
    ("Linux".into(), "Unknown".into())
}

/// `os_info::Type` display names for the distributions a proxy is likely to
/// run on, from `/etc/os-release`; anything else is plain `Linux`.
#[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
fn os_name_and_version() -> (String, String) {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |name: &str| {
        release
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(|value| value.trim().trim_matches('"').to_owned())
            .filter(|value| !value.is_empty())
    };
    let name = match field("ID").as_deref() {
        Some("ubuntu") => "Ubuntu",
        Some("debian") => "Debian",
        Some("raspbian") => "Raspbian",
        Some("fedora") => "Fedora",
        Some("arch") => "Arch Linux",
        Some("alpine") => "Alpine Linux",
        Some("centos") => "CentOS",
        Some("rocky") => "Rocky Linux",
        Some("almalinux") => "AlmaLinux",
        Some("rhel") => "Red Hat Enterprise Linux",
        Some("amzn") => "Amazon Linux AMI",
        Some("nixos") => "NixOS",
        Some("manjaro") => "Manjaro",
        Some("linuxmint") => "Linux Mint",
        Some("pop") => "Pop!_OS",
        Some("opensuse-leap" | "opensuse-tumbleweed" | "opensuse") => "openSUSE",
        _ => "Linux",
    };
    (
        name.into(),
        field("VERSION_ID").unwrap_or_else(|| "Unknown".into()),
    )
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
fn os_name_and_version() -> (String, String) {
    let plist = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
        .unwrap_or_default();
    let version = plist
        .split("<key>ProductVersion</key>")
        .nth(1)
        .and_then(|rest| rest.split("<string>").nth(1))
        .and_then(|rest| rest.split("</string>").next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown");
    ("Mac OS".into(), version.into())
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "linux", target_os = "macos"))
))]
fn os_name_and_version() -> (String, String) {
    let name = match std::env::consts::OS {
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        "android" => "Android",
        _ => "Unknown",
    };
    (name.into(), "Unknown".into())
}

/// `os_info` reports `uname -m` on Unix (`arm64` on Apple silicon) and
/// `PROCESSOR_ARCHITECTURE` on Windows.
fn architecture() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "arm64",
        ("windows", "x86_64") => "AMD64",
        ("windows", "aarch64") => "ARM64",
        (_, arch) => arch,
    }
}

#[cfg(target_arch = "wasm32")]
fn terminal() -> String {
    "unknown".into()
}

#[cfg(not(target_arch = "wasm32"))]
fn terminal() -> String {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let raw = match var("TERM_PROGRAM").filter(|program| !program.eq_ignore_ascii_case("tmux")) {
        Some(program) => match var("TERM_PROGRAM_VERSION") {
            Some(version) => format!("{program}/{version}"),
            None => program,
        },
        None => var("TERM").unwrap_or_else(|| "unknown".into()),
    };
    raw.chars()
        .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
        .collect()
}
