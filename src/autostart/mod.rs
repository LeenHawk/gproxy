//! Per-user login auto-start for native desktop builds.
//!
//! Only the executable, working directory, and non-secret CLI arguments are
//! persisted. Android uses its APK foreground service; containers are skipped.

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::PathBuf;

use serde::Serialize;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

const INIT_MARKER: &str = ".autostart-initialized";

#[derive(Debug, Clone, Serialize)]
pub struct AutoStartStatus {
    pub supported: bool,
    pub enabled: bool,
    pub platform: &'static str,
    pub detail: Option<String>,
}

#[derive(Debug)]
pub struct AutoStartManager {
    data_dir: PathBuf,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    executable: PathBuf,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    args: Vec<OsString>,
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    working_dir: PathBuf,
    blocked: Option<String>,
}

impl AutoStartManager {
    pub fn for_current_process(data_dir: PathBuf) -> Self {
        let raw_args = std::env::args_os().skip(1).collect::<Vec<_>>();
        let blocked = sensitive_configuration(&raw_args);
        Self {
            data_dir: std::path::absolute(&data_dir).unwrap_or(data_dir),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            executable: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gproxy")),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            args: safe_serve_args(raw_args.into_iter()),
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            blocked,
        }
    }

    /// Apply the desktop default once per data directory. Operators can opt
    /// out before first boot with `GPROXY_AUTOSTART=off`.
    pub fn initialize_default(&self) -> anyhow::Result<AutoStartStatus> {
        let status = self.status();
        if !status.supported || self.data_dir.join(INIT_MARKER).exists() {
            return Ok(status);
        }
        let enabled = match std::env::var("GPROXY_AUTOSTART") {
            Ok(value) => parse_bool(&value)?,
            Err(_) => true,
        };
        self.set_enabled(enabled)
    }

    pub fn status(&self) -> AutoStartStatus {
        let mut status = platform::status(self);
        if let Some(reason) = &self.blocked {
            status.supported = false;
            status.detail = Some(reason.clone());
        }
        status
    }

    pub fn set_enabled(&self, enabled: bool) -> anyhow::Result<AutoStartStatus> {
        let platform_status = platform::status(self);
        if !platform_status.supported {
            anyhow::bail!(
                "automatic startup is unavailable: {}",
                platform_status
                    .detail
                    .unwrap_or_else(|| "unsupported platform".into())
            );
        }
        if enabled && let Some(reason) = &self.blocked {
            anyhow::bail!("automatic startup is unavailable: {reason}");
        }
        platform::set_enabled(self, enabled)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::write(self.data_dir.join(INIT_MARKER), b"1\n")?;
        Ok(self.status())
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    fn command_parts(&self) -> impl Iterator<Item = &OsStr> {
        std::iter::once(self.executable.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
    }
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enable" | "enabled" => Ok(true),
        "0" | "false" | "no" | "off" | "disable" | "disabled" => Ok(false),
        _ => anyhow::bail!("GPROXY_AUTOSTART must be on/off or true/false"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn safe_serve_args(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut safe = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        let text = arg.to_string_lossy();
        if matches!(
            text.as_ref(),
            "--admin-password" | "--dsn" | "--redis-url" | "--upstream-proxy-url"
        ) {
            skip_next = true;
        } else if [
            "--admin-password=",
            "--dsn=",
            "--redis-url=",
            "--upstream-proxy-url=",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        {
            continue;
        } else if matches!(text.as_ref(), "import" | "export" | "migrate-v1" | "update") {
            break;
        } else {
            safe.push(arg);
        }
    }
    safe
}

fn sensitive_configuration(args: &[OsString]) -> Option<String> {
    let sensitive_args = [
        "--admin-password",
        "--dsn",
        "--redis-url",
        "--upstream-proxy-url",
    ];
    let has_sensitive_arg = args.iter().any(|arg| {
        let text = arg.to_string_lossy();
        sensitive_args.iter().any(|name| {
            text == *name
                || text
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with('='))
        })
    });
    let sensitive_env = [
        "GPROXY_ADMIN_PASSWORD",
        "GPROXY_DSN",
        "GPROXY_REDIS_URL",
        "GPROXY_UPSTREAM_PROXY_URL",
        "GPROXY_MASTER_KEY",
        "GPROXY_IMPORT_FILE",
    ];
    if has_sensitive_arg
        || sensitive_env
            .iter()
            .any(|name| std::env::var_os(name).is_some())
    {
        Some(
            "use an external service manager when startup depends on secrets or external configuration"
                .into(),
        )
    } else {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::*;

    pub(super) fn status(_manager: &AutoStartManager) -> AutoStartStatus {
        AutoStartStatus {
            supported: false,
            enabled: false,
            platform: std::env::consts::OS,
            detail: Some("managed by the platform launcher or unsupported".into()),
        }
    }

    pub(super) fn set_enabled(_manager: &AutoStartManager, _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("automatic startup is managed by the platform launcher")
    }
}

#[cfg(test)]
mod tests;
