#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use base64::Engine as _;
use serde::Deserialize;

use crate::ConfigError;

#[cfg(not(target_arch = "wasm32"))]
mod native;

#[derive(Clone)]
pub struct Config {
    #[cfg(not(target_arch = "wasm32"))]
    listen_addr: SocketAddr,
    #[cfg(not(target_arch = "wasm32"))]
    data_dir: PathBuf,
    backend: StoreBackend,
    master_key: [u8; 32],
}

#[derive(Clone)]
enum StoreBackend {
    #[cfg(not(target_arch = "wasm32"))]
    Sqlite,
    Libsql {
        url: String,
        auth_token: String,
    },
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawConfig {
    #[cfg(not(target_arch = "wasm32"))]
    listen_addr: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    data_dir: Option<String>,
    store_backend: Option<String>,
    libsql_url: Option<String>,
    libsql_auth_token: Option<String>,
    secret_key: Option<String>,
}

impl Config {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let source = std::fs::read_to_string(path.as_ref())
            .map_err(|error| ConfigError::Read(error.to_string()))?;
        let mut raw = parse(&source)?;
        raw.apply_env()?;
        Self::validate(raw)
    }

    pub fn from_toml(source: &str) -> Result<Self, ConfigError> {
        Self::validate(parse(source)?)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn backend_config(&self) -> gproxy_store::BackendConfig {
        match &self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Sqlite => gproxy_store::BackendConfig::Sqlite {
                path: self.data_dir.join("gproxy.db"),
            },
            StoreBackend::Libsql { url, auth_token } => gproxy_store::BackendConfig::Libsql {
                url: url.clone(),
                auth_token: auth_token.clone(),
            },
        }
    }

    pub(crate) fn master_key(&self) -> &[u8; 32] {
        &self.master_key
    }

    fn validate(raw: RawConfig) -> Result<Self, ConfigError> {
        #[cfg(not(target_arch = "wasm32"))]
        let listen = required(raw.listen_addr, "listen_addr")?;
        #[cfg(not(target_arch = "wasm32"))]
        let listen_addr = listen.parse().map_err(|error| ConfigError::Invalid {
            field: "listen_addr",
            message: format!("expected IP socket address: {error}"),
        })?;
        #[cfg(not(target_arch = "wasm32"))]
        let data_dir = PathBuf::from(required(raw.data_dir, "data_dir")?);
        let backend = match required(raw.store_backend, "store_backend")?.as_str() {
            "sqlite" => {
                #[cfg(target_arch = "wasm32")]
                return Err(invalid("store_backend", "sqlite is unavailable on wasm"));
                #[cfg(not(target_arch = "wasm32"))]
                StoreBackend::Sqlite
            }
            "libsql" => StoreBackend::Libsql {
                url: libsql_url(required(raw.libsql_url, "libsql_url")?)?,
                auth_token: required(raw.libsql_auth_token, "libsql_auth_token")?,
            },
            _ => return Err(invalid("store_backend", "expected `sqlite` or `libsql`")),
        };
        let encoded = required(raw.secret_key, "secret_key")?;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| invalid("secret_key", "must be standard base64"))?;
        let master_key = decoded.try_into().map_err(|bytes: Vec<u8>| {
            invalid(
                "secret_key",
                format!("must decode to 32 bytes, got {}", bytes.len()),
            )
        })?;
        Ok(Self {
            #[cfg(not(target_arch = "wasm32"))]
            listen_addr,
            #[cfg(not(target_arch = "wasm32"))]
            data_dir,
            backend,
            master_key,
        })
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backend = match &self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Sqlite => "Sqlite".to_owned(),
            StoreBackend::Libsql { url, .. } => format!("Libsql {{ url: {url:?} }}"),
        };
        let mut debug = formatter.debug_struct("Config");
        #[cfg(not(target_arch = "wasm32"))]
        debug
            .field("listen_addr", &self.listen_addr)
            .field("data_dir", &self.data_dir);
        debug
            .field("backend", &backend)
            .field("master_key", &"<redacted>")
            .finish()
    }
}

fn parse(source: &str) -> Result<RawConfig, ConfigError> {
    toml::from_str(source)
        .map_err(|error: toml::de::Error| ConfigError::Parse(error.message().into()))
}

fn required(value: Option<String>, field: &'static str) -> Result<String, ConfigError> {
    match value {
        Some(value) if !value.trim().is_empty() => Ok(value),
        Some(_) => Err(invalid(field, "must not be empty")),
        None => Err(invalid(field, "required")),
    }
}

fn libsql_url(value: String) -> Result<String, ConfigError> {
    let uri: http::Uri = value
        .parse()
        .map_err(|_| invalid("libsql_url", "must be an absolute HTTP URL"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
        return Err(invalid("libsql_url", "must be an absolute HTTP URL"));
    }
    Ok(value)
}

fn invalid(field: &'static str, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        field,
        message: message.into(),
    }
}
