mod android_apk;
mod config;
mod download;
mod extract;
mod manifest;
mod notes;
mod swap;
mod version;

use std::path::PathBuf;

use bytes::Bytes;
use gproxy_admin::dto::{UpdateAppliedDto, UpdateStatusDto};
use http::{Method, Response, StatusCode};

pub(crate) use config::Error;
use config::{Restart, channel, restart};

type Result<T> = std::result::Result<T, Error>;

pub(crate) struct Manager {
    client: wreq::Client,
    data_dir: PathBuf,
    channel: &'static str,
    manifest_url: String,
    restart: Restart,
}

impl Manager {
    pub(crate) fn new(data_dir: PathBuf, proxy: Option<&str>) -> Result<Self> {
        let channel = channel()?;
        let manifest_url = std::env::var("GPROXY_UPDATE_SERVE").unwrap_or_else(|_| {
            if channel == "staging" {
                "https://github.com/LeenHawk/gproxy/releases/download/staging/manifest.json".into()
            } else {
                "https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json".into()
            }
        });
        let mut builder = wreq::Client::builder()
            .user_agent(concat!("gproxy-selfupdate/", env!("CARGO_PKG_VERSION")));
        builder = match proxy {
            Some(url) => builder.proxy(wreq::Proxy::all(url).map_err(|_| Error::Configuration)?),
            None => builder.no_proxy(),
        };
        Ok(Self {
            client: builder.build().map_err(|_| Error::Configuration)?,
            data_dir,
            channel,
            manifest_url,
            restart: restart()?,
        })
    }

    async fn check(&self) -> Result<UpdateStatusDto> {
        let manifest = download::manifest(&self.client, &self.manifest_url).await?;
        if manifest.channel != self.channel {
            return Err(Error::Manifest);
        }
        let target = version::target();
        let _artifact = manifest.artifact(&target)?;
        let (current, available) = version::available(self.channel, &manifest.version)?;
        let notes = if available && self.channel == "releases" && manifest.notes_url.is_some() {
            notes::fetch(&self.client, &manifest.version).await
        } else {
            None
        };
        Ok(UpdateStatusDto {
            current,
            latest: manifest.version,
            available,
            channel: self.channel.into(),
            target,
            notes,
            rollback_available: swap::rollback_available(),
            restart: self.restart.as_str().into(),
        })
    }

    async fn apply(&self) -> Result<(UpdateAppliedDto, bool)> {
        let manifest = download::manifest(&self.client, &self.manifest_url).await?;
        if manifest.channel != self.channel {
            return Err(Error::Manifest);
        }
        version::compatible(
            manifest.min_compatible_data_version,
            gproxy_store::schema::SchemaVersion::LATEST.number() as u32,
        )?;
        let (_, available) = version::available(self.channel, &manifest.version)?;
        if available {
            let target = version::target();
            let bytes = download::artifact(&self.client, manifest.artifact(&target)?).await?;
            if target.ends_with("-apk") {
                android_apk::stage(&self.data_dir, &bytes)?;
            } else {
                let staged = extract::binary(&bytes, &self.data_dir.join(".update"), &target)?;
                swap::install(&staged)?;
            }
        }
        Ok((
            UpdateAppliedDto {
                version: manifest.version,
                restart: self.restart.as_str().into(),
            },
            available,
        ))
    }

    pub(crate) async fn dispatch(&self, method: &Method, path: &str) -> Response<Bytes> {
        let (result, restart_after) = match (method, path) {
            (&Method::GET | &Method::HEAD, "/admin/api/native/update") => {
                (self.check().await.and_then(to_value), false)
            }
            (&Method::POST, "/admin/api/native/update/apply") => match self.apply().await {
                Ok((applied, changed)) => (to_value(applied), changed),
                Err(error) => (Err(error), false),
            },
            (&Method::POST, "/admin/api/native/update/rollback") => (
                swap::rollback().map(|_| {
                    serde_json::json!({ "version": crate::BUILD_VERSION, "restart": self.restart.as_str() })
                }),
                true,
            ),
            _ => return json(StatusCode::METHOD_NOT_ALLOWED, serde_json::json!({})),
        };
        match result {
            Ok(value) => {
                if restart_after {
                    schedule_restart(self.restart);
                }
                json(StatusCode::OK, value)
            }
            Err(error) => json(
                error.status(),
                serde_json::json!({"error": {"message": error.to_string()}}),
            ),
        }
    }
}

fn schedule_restart(restart: Restart) {
    if matches!(restart, Restart::None) {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        match restart {
            Restart::None => {}
            Restart::Supervisor => std::process::exit(42),
            Restart::ReExec => reexec(),
        }
    });
}

#[cfg(unix)]
fn reexec() -> ! {
    use std::os::unix::process::CommandExt as _;
    let executable = std::env::current_exe().unwrap_or_else(|_| std::process::exit(1));
    let error = std::process::Command::new(executable)
        .args(std::env::args_os().skip(1))
        .exec();
    tracing::error!(%error, "updated process re-exec failed");
    std::process::exit(1)
}

#[cfg(not(unix))]
fn reexec() -> ! {
    std::process::exit(42)
}

fn to_value(value: impl serde::Serialize) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|_| Error::Manifest)
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

pub(crate) fn unavailable() -> Response<Bytes> {
    json(
        StatusCode::SERVICE_UNAVAILABLE,
        serde_json::json!({"error": {"message": Error::Configuration.to_string()}}),
    )
}
