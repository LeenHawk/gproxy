use http::StatusCode;

use super::Result;

pub(super) fn channel(selected: Option<&str>, configured: Option<&str>) -> Result<String> {
    let value = configured
        .map(str::to_owned)
        .or_else(|| {
            std::env::var("GPROXY_UPDATE_CHANNEL_SERVE")
                .or_else(|_| std::env::var("GPROXY_UPDATE_CHANNEL"))
                .ok()
        })
        .or_else(|| selected.map(str::to_owned))
        .unwrap_or_else(|| crate::BUILD_CHANNEL.into());
    match value.to_ascii_lowercase().as_str() {
        "releases" | "release" | "stable" => Ok("releases".into()),
        "staging" => Ok("staging".into()),
        "dev" | "development" => Ok("dev".into()),
        _ => Err(Error::Configuration),
    }
}

pub(super) fn restart() -> Result<Restart> {
    let value = std::env::var("GPROXY_UPDATE_RESTART").ok();
    parse_restart(value.as_deref())
}

fn parse_restart(value: Option<&str>) -> Result<Restart> {
    match value.unwrap_or("re-exec").to_ascii_lowercase().as_str() {
        "none" => Ok(Restart::None),
        "supervisor" => Ok(Restart::Supervisor),
        "re-exec" | "reexec" => Ok(Restart::ReExec),
        _ => Err(Error::Configuration),
    }
}

#[derive(Clone, Copy)]
pub(super) enum Restart {
    None,
    Supervisor,
    ReExec,
}

impl Restart {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Supervisor => "supervisor",
            Self::ReExec => "re-exec",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("self-update configuration is invalid")]
    Configuration,
    #[error("signed update manifest is unavailable or invalid")]
    Manifest,
    #[error("update manifest signature verification failed")]
    Signature,
    #[error("this release has no artifact for the running target")]
    Artifact,
    #[error("update download failed")]
    Download,
    #[error("downloaded update failed its integrity check")]
    Integrity,
    #[error("verified update archive is invalid")]
    Archive,
    #[error("update is incompatible with the current data version")]
    Incompatible,
    #[error("update version is invalid")]
    Version,
    #[error("executable swap failed")]
    Swap,
    #[error("no rollback executable is available")]
    Rollback,
    #[error("update filesystem operation failed")]
    Io(#[from] std::io::Error),
}

impl Error {
    pub(super) fn status(&self) -> StatusCode {
        match self {
            Self::Incompatible | Self::Version | Self::Rollback => StatusCode::CONFLICT,
            Self::Configuration => StatusCode::BAD_REQUEST,
            _ => StatusCode::BAD_GATEWAY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_restart;

    #[test]
    fn restart_defaults_to_reexec_but_allows_an_explicit_override() {
        assert_eq!(parse_restart(None).unwrap().as_str(), "re-exec");
        assert_eq!(parse_restart(Some("none")).unwrap().as_str(), "none");
        assert_eq!(
            parse_restart(Some("supervisor")).unwrap().as_str(),
            "supervisor"
        );
    }
}
