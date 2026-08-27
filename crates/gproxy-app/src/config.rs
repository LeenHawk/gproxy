#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddr;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use base64::Engine as _;

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
    secret_keys: SecretKeyConfig,
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

#[derive(Clone)]
pub struct SecretKeyConfig {
    pub(crate) current: Option<[u8; 32]>,
    pub(crate) next: RotationTarget,
    pub(crate) rotate: bool,
}

#[derive(Clone)]
pub(crate) enum RotationTarget {
    Unset,
    Plaintext,
    Key([u8; 32]),
}

impl SecretKeyConfig {
    pub fn new(current: Option<[u8; 32]>) -> Self {
        Self {
            current,
            next: RotationTarget::Unset,
            rotate: false,
        }
    }

    pub fn from_encoded(
        current: Option<String>,
        next: Option<String>,
        rotate: bool,
    ) -> Result<Self, ConfigError> {
        let current = current
            .map(|value| decode_key(&value, "GPROXY_SECRET_KEY"))
            .transpose()?;
        let next = match next {
            None => RotationTarget::Unset,
            Some(value) if value.is_empty() => RotationTarget::Plaintext,
            Some(value) => RotationTarget::Key(decode_key(&value, "GPROXY_SECRET_KEY_NEXT")?),
        };
        Ok(Self {
            current,
            next,
            rotate,
        })
    }

    pub fn rotate_to_key(mut self, next: [u8; 32]) -> Self {
        self.next = RotationTarget::Key(next);
        self.rotate = true;
        self
    }

    pub fn rotate_to_plaintext(mut self) -> Self {
        self.next = RotationTarget::Plaintext;
        self.rotate = true;
        self
    }
}

impl Config {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_env() -> Result<Self, ConfigError> {
        native::load()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn sqlite(
        listen_addr: SocketAddr,
        data_dir: PathBuf,
        secret_keys: SecretKeyConfig,
    ) -> Self {
        Self {
            listen_addr,
            data_dir,
            backend: StoreBackend::Sqlite,
            secret_keys,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn libsql(
        listen_addr: SocketAddr,
        data_dir: PathBuf,
        url: String,
        auth_token: String,
        secret_keys: SecretKeyConfig,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            listen_addr,
            data_dir,
            backend: StoreBackend::Libsql {
                url: libsql_url(url)?,
                auth_token: required(auth_token, "GPROXY_LIBSQL_AUTH_TOKEN")?,
            },
            secret_keys,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn libsql(
        url: String,
        auth_token: String,
        secret_keys: SecretKeyConfig,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            backend: StoreBackend::Libsql {
                url: libsql_url(url)?,
                auth_token: required(auth_token, "GPROXY_LIBSQL_AUTH_TOKEN")?,
            },
            secret_keys,
        })
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

    pub(crate) fn secret_keys(&self) -> &SecretKeyConfig {
        &self.secret_keys
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
            .field("secret_keys", &"<redacted>")
            .finish()
    }
}

fn decode_key(value: &str, field: &'static str) -> Result<[u8; 32], ConfigError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| invalid(field, "must be standard base64"))?;
    decoded.try_into().map_err(|bytes: Vec<u8>| {
        invalid(
            field,
            format!("must decode to 32 bytes, got {}", bytes.len()),
        )
    })
}

fn required(value: String, field: &'static str) -> Result<String, ConfigError> {
    if value.trim().is_empty() {
        Err(invalid(field, "required and must not be empty"))
    } else {
        Ok(value)
    }
}

fn libsql_url(value: String) -> Result<String, ConfigError> {
    let value = required(value, "GPROXY_LIBSQL_URL")?;
    let uri: http::Uri = value
        .parse()
        .map_err(|_| invalid("GPROXY_LIBSQL_URL", "must be an absolute HTTP URL"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
        return Err(invalid("GPROXY_LIBSQL_URL", "must be an absolute HTTP URL"));
    }
    Ok(value)
}

fn invalid(field: &'static str, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        field,
        message: message.into(),
    }
}
