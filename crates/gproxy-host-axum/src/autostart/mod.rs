use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use bytes::Bytes;
use http::{Method, Response, StatusCode};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
use android as platform;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
// Compiled off-target so the generated boot script stays under test.
#[cfg(any(target_os = "android", test))]
mod termux;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

const INIT_MARKER: &str = ".autostart-initialized";

pub(crate) type Status = gproxy_admin::dto::AutostartStatusDto;

pub(crate) struct Manager {
    data_dir: PathBuf,
    pub(super) executable: PathBuf,
    pub(super) args: Vec<OsString>,
    pub(super) working_dir: PathBuf,
}

impl Manager {
    pub(crate) fn for_current_process(data_dir: PathBuf) -> Self {
        let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
        carry_master_key(&mut args);
        Self {
            data_dir: std::path::absolute(&data_dir).unwrap_or(data_dir),
            executable: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("gproxy")),
            args,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    pub(crate) fn initialize_default(&self) -> Result<Status, Error> {
        let status = self.status();
        if !status.supported || self.data_dir.join(INIT_MARKER).exists() {
            return Ok(status);
        }
        let enabled = std::env::var("GPROXY_AUTOSTART")
            .ok()
            .map(|value| parse_bool(&value))
            .transpose()?
            .unwrap_or(true);
        self.set_enabled(enabled)
    }

    pub(crate) fn status(&self) -> Status {
        platform::status(self)
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<Status, Error> {
        let status = self.status();
        if !status.supported {
            return Err(Error::Unsupported);
        }
        platform::set_enabled(self, enabled)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::write(self.data_dir.join(INIT_MARKER), b"1\n")?;
        Ok(self.status())
    }

    pub(super) fn command_parts(&self) -> impl Iterator<Item = &OsStr> {
        std::iter::once(self.executable.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
    }
}

fn carry_master_key(args: &mut Vec<OsString>) {
    let supplied = args.iter().any(|arg| {
        let value = arg.to_string_lossy();
        value == "--master-key" || value.starts_with("--master-key=")
    });
    if !supplied && let Some(value) = std::env::var_os("GPROXY_MASTER_KEY") {
        args.push("--master-key".into());
        args.push(value);
    }
}

fn parse_bool(value: &str) -> Result<bool, Error> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enable" | "enabled" => Ok(true),
        "0" | "false" | "no" | "off" | "disable" | "disabled" => Ok(false),
        _ => Err(Error::InvalidSetting),
    }
}

pub(crate) fn dispatch(manager: Option<&Manager>, method: &Method, body: &[u8]) -> Response<Bytes> {
    let result = match (manager, method) {
        (Some(manager), &Method::GET | &Method::HEAD) => Ok(manager.status()),
        (Some(manager), &Method::PUT) => {
            serde_json::from_slice::<gproxy_admin::dto::AutostartUpdateRequest>(body)
                .map_err(|_| Error::InvalidSetting)
                .and_then(|request| manager.set_enabled(request.enabled))
        }
        (None, _) => Err(Error::Unsupported),
        _ => return json(StatusCode::METHOD_NOT_ALLOWED, serde_json::json!({})),
    };
    match result {
        Ok(status) => json(StatusCode::OK, serde_json::json!(status)),
        Err(error) => json(
            if matches!(error, Error::InvalidSetting) {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::CONFLICT
            },
            serde_json::json!({"error": {"message": error.to_string()}}),
        ),
    }
}

fn json(status: StatusCode, value: serde_json::Value) -> Response<Bytes> {
    let mut response = Response::new(Bytes::from(value.to_string()));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    response
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("automatic startup is unavailable on this installation")]
    Unsupported,
    #[error("GPROXY_AUTOSTART must be on/off or true/false")]
    InvalidSetting,
    #[error("automatic startup filesystem operation failed")]
    Io(#[from] std::io::Error),
}

#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    windows
)))]
mod platform {
    use super::*;

    pub(super) fn status(_manager: &Manager) -> Status {
        Status {
            supported: false,
            enabled: false,
            platform: std::env::consts::OS.into(),
            detail: Some("platform".into()),
        }
    }

    pub(super) fn set_enabled(_manager: &Manager, _enabled: bool) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}
