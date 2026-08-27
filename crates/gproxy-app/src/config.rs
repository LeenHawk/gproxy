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
    cache: CacheConfig,
    secret_keys: MasterKeyConfig,
    #[cfg(not(target_arch = "wasm32"))]
    native: NativeOptions,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub(crate) struct NativeOptions {
    pub upstream_proxy_url: Option<String>,
    pub instance_id: u64,
    pub max_attempts: u32,
    pub max_in_flight: usize,
    pub file_upload_max_in_flight: Option<usize>,
    pub trusted_proxies: Vec<std::net::IpAddr>,
    pub cors_origins: Vec<String>,
    pub log_format: LogFormat,
    pub admin_user: String,
    pub admin_password: Option<String>,
    pub bootstrap_admin_api_key: Option<String>,
    pub bootstrap_channels: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            upstream_proxy_url: None,
            instance_id: 0,
            max_attempts: 6,
            max_in_flight: 1024,
            file_upload_max_in_flight: None,
            trusted_proxies: Vec::new(),
            cors_origins: Vec::new(),
            log_format: LogFormat::Text,
            admin_user: "admin".into(),
            admin_password: None,
            bootstrap_admin_api_key: None,
            bootstrap_channels: Vec::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

#[cfg(not(target_arch = "wasm32"))]
pub enum NativeCommand {
    Serve(Config),
    MigrateV2 {
        config: Config,
        options: crate::V2ImportOptions,
    },
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeCommand {
    pub fn from_env() -> Result<Self, ConfigError> {
        native::load()
    }
}

#[derive(Clone)]
enum StoreBackend {
    #[cfg(not(target_arch = "wasm32"))]
    Sqlite,
    #[cfg(not(target_arch = "wasm32"))]
    Postgres {
        dsn: String,
    },
    #[cfg(not(target_arch = "wasm32"))]
    Mysql {
        dsn: String,
    },
    Libsql {
        url: String,
        auth_token: String,
    },
}

#[derive(Clone)]
pub(crate) enum CacheConfig {
    #[cfg(not(target_arch = "wasm32"))]
    InProcess,
    #[cfg(not(target_arch = "wasm32"))]
    Redis {
        url: String,
    },
    Libsql,
    Upstash {
        url: String,
        token: String,
    },
}

#[derive(Clone)]
pub struct MasterKeyConfig {
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

impl MasterKeyConfig {
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
            .map(|value| decode_key(&value, "GPROXY_MASTER_KEY"))
            .transpose()?;
        let next = match next {
            None => RotationTarget::Unset,
            Some(value) if value.is_empty() => RotationTarget::Plaintext,
            Some(value) => RotationTarget::Key(decode_key(&value, "GPROXY_MASTER_KEY_NEXT")?),
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
        match native::load()? {
            NativeCommand::Serve(config) => Ok(config),
            NativeCommand::MigrateV2 { .. } => Err(invalid(
                "command",
                "migration commands must be handled by the native host",
            )),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn sqlite(
        listen_addr: SocketAddr,
        data_dir: PathBuf,
        secret_keys: MasterKeyConfig,
    ) -> Self {
        Self {
            listen_addr,
            data_dir,
            backend: StoreBackend::Sqlite,
            cache: CacheConfig::InProcess,
            secret_keys,
            native: NativeOptions::default(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn libsql(
        listen_addr: SocketAddr,
        data_dir: PathBuf,
        url: String,
        auth_token: String,
        secret_keys: MasterKeyConfig,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            listen_addr,
            data_dir,
            backend: StoreBackend::Libsql {
                url: libsql_url(url)?,
                auth_token: required(auth_token, "GPROXY_LIBSQL_AUTH_TOKEN")?,
            },
            cache: CacheConfig::Libsql,
            secret_keys,
            #[cfg(not(target_arch = "wasm32"))]
            native: NativeOptions::default(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn libsql(
        url: String,
        auth_token: String,
        secret_keys: MasterKeyConfig,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            backend: StoreBackend::Libsql {
                url: libsql_url(url)?,
                auth_token: required(auth_token, "GPROXY_LIBSQL_AUTH_TOKEN")?,
            },
            cache: CacheConfig::Libsql,
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn upstream_proxy_url(&self) -> Option<&str> {
        self.native.upstream_proxy_url.as_deref()
    }

    pub fn backend_config(&self) -> gproxy_store::BackendConfig {
        match &self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Sqlite => gproxy_store::BackendConfig::Sqlite {
                path: self.data_dir.join("gproxy.db"),
            },
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Postgres { dsn } => {
                gproxy_store::BackendConfig::Postgres { dsn: dsn.clone() }
            }
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Mysql { dsn } => gproxy_store::BackendConfig::Mysql { dsn: dsn.clone() },
            StoreBackend::Libsql { url, auth_token } => gproxy_store::BackendConfig::Libsql {
                url: url.clone(),
                auth_token: auth_token.clone(),
            },
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn sql_server(
        mut self,
        backend: &'static str,
        dsn: String,
    ) -> Result<Self, ConfigError> {
        let dsn = required(dsn, "GPROXY_DSN")?;
        self.backend = match backend {
            "postgres" => StoreBackend::Postgres { dsn },
            "mysql" => StoreBackend::Mysql { dsn },
            _ => return Err(invalid("GPROXY_PERSISTENCE", "unsupported SQL backend")),
        };
        Ok(self)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_cache(mut self, cache: CacheConfig) -> Self {
        self.cache = cache;
        self
    }

    pub fn with_upstash(mut self, url: String, token: String) -> Result<Self, ConfigError> {
        self.cache = CacheConfig::Upstash {
            url: absolute_http_url(url, "UPSTASH_URL")?,
            token: required(token, "UPSTASH_TOKEN")?,
        };
        Ok(self)
    }

    pub(crate) fn cache(&self) -> &CacheConfig {
        &self.cache
    }

    pub(crate) fn secret_keys(&self) -> &MasterKeyConfig {
        &self.secret_keys
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn with_native_options(mut self, native: NativeOptions) -> Self {
        self.native = native;
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native(&self) -> &NativeOptions {
        &self.native
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn max_in_flight(&self) -> usize {
        self.native.max_in_flight
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn trusted_proxies(&self) -> &[std::net::IpAddr] {
        &self.native.trusted_proxies
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn cors_origins(&self) -> &[String] {
        &self.native.cors_origins
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn log_format(&self) -> LogFormat {
        self.native.log_format
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn instance_id(&self) -> u64 {
        self.native.instance_id
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backend = match &self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Sqlite => "Sqlite".to_owned(),
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Postgres { .. } => "Postgres { dsn: <redacted> }".to_owned(),
            #[cfg(not(target_arch = "wasm32"))]
            StoreBackend::Mysql { .. } => "Mysql { dsn: <redacted> }".to_owned(),
            StoreBackend::Libsql { url, .. } => format!("Libsql {{ url: {url:?} }}"),
        };
        let mut debug = formatter.debug_struct("Config");
        #[cfg(not(target_arch = "wasm32"))]
        debug
            .field("listen_addr", &self.listen_addr)
            .field("data_dir", &self.data_dir);
        debug
            .field("backend", &backend)
            .field(
                "cache",
                &match self.cache {
                    #[cfg(not(target_arch = "wasm32"))]
                    CacheConfig::InProcess => "InProcess",
                    #[cfg(not(target_arch = "wasm32"))]
                    CacheConfig::Redis { .. } => "Redis { url: <redacted> }",
                    CacheConfig::Libsql => "Libsql",
                    CacheConfig::Upstash { .. } => "Upstash { credentials: <redacted> }",
                },
            )
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
    absolute_http_url(value, "GPROXY_LIBSQL_URL")
}

fn absolute_http_url(value: String, field: &'static str) -> Result<String, ConfigError> {
    let value = required(value, field)?;
    let uri: http::Uri = value
        .parse()
        .map_err(|_| invalid(field, "must be an absolute HTTP URL"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
        return Err(invalid(field, "must be an absolute HTTP URL"));
    }
    Ok(value)
}

fn invalid(field: &'static str, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        field,
        message: message.into(),
    }
}
