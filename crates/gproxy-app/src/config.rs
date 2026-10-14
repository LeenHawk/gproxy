//! What an instance is configured with, as plain data.
//!
//! This type does not read files, the environment or the filesystem, and it
//! never opens anything. The binary layers `clap` → env → `.env` → a TOML file
//! → these defaults into one `AppConfig` and then constructs backends from it;
//! the edge host deserializes the same shape out of its Worker bindings. That
//! is why everything here is `Deserialize + Serialize` with a default for every
//! field: the two hosts must agree on one configuration vocabulary, and a
//! config file may set only what it wants to change.
//!
//! Paths are kept as strings rather than `PathBuf` so the type stays usable on
//! `wasm32`, where there is no filesystem to resolve them against.

use crate::AppError;
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Thirty days, the default lifetime of a console/portal session and of an
/// issued OAuth refresh token.
const THIRTY_DAYS_SECS: u64 = 30 * 24 * 3600;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// Listen address of a native host. Ignored by the edge host, which is
    /// given its socket by the runtime.
    pub host: String,
    pub port: u16,
    /// Root for instance state a backend resolves relative paths against: the
    /// SQLite file, the local file-storage root, downloaded vocabularies.
    pub data_dir: Option<String>,
    pub store: StoreBackendConfig,
    pub cache: CacheBackendConfig,
    pub master_key: MasterKeyConfig,
    /// None keeps file-backed resources (publications, vocabularies) disabled;
    /// core refuses those operations rather than inventing a location.
    pub file_storage: Option<FileStorageConfig>,
    /// Absolute external origin, e.g. `https://gproxy.example.com`. Used to
    /// mint publication URLs and the OAuth issuer identifier when the request
    /// itself cannot be trusted to name the instance.
    pub public_base_url: Option<String>,
    /// Browser origins allowed to call the APIs. Empty means same-origin only.
    pub cors_origins: Vec<String>,
    /// Peers whose `x-forwarded-for` / `x-forwarded-proto` are believed. Empty
    /// means the socket address is the client address, which is the safe
    /// default: a spoofed client IP is a spoofed rate-limit bucket.
    pub trusted_proxies: Vec<String>,
    pub console: ConsoleConfig,
    /// Record management and OAuth audit events. Read at startup; existing rows remain queryable.
    pub audit_enabled: bool,
    pub session_ttl_secs: u64,
    pub oauth: OAuthIssuerConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            data_dir: None,
            store: StoreBackendConfig::default(),
            cache: CacheBackendConfig::default(),
            master_key: MasterKeyConfig::default(),
            file_storage: None,
            public_base_url: None,
            cors_origins: Vec::new(),
            trusted_proxies: Vec::new(),
            console: ConsoleConfig::default(),
            audit_enabled: true,
            session_ttl_secs: THIRTY_DAYS_SECS,
            oauth: OAuthIssuerConfig::default(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".into()
}

fn default_port() -> u16 {
    8787
}

/// Where the durable configuration and usage live. Which variants a build can
/// actually open depends on the backend features the binary was built with;
/// an unsupported one is refused when the connection is made, not here.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoreBackendConfig {
    /// A local file, resolved against `data_dir` when relative.
    Sqlite { path: String },
    /// Any SeaORM connection string (`postgres://`, `mysql://`, `sqlite://`).
    Url { dsn: String },
    /// A Cloudflare D1 binding name, resolved from the Worker environment.
    D1 { binding: String },
    Libsql {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
    },
}

impl Default for StoreBackendConfig {
    fn default() -> Self {
        Self::Sqlite {
            path: "gproxy.db".into(),
        }
    }
}

/// Shared TTL state, counters and invalidation notices. `Memory` is per
/// process: correct for a single instance, and silently wrong for several,
/// which is why a multi-instance deployment must name `Redis` or `Store`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CacheBackendConfig {
    #[default]
    Memory,
    Redis {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
    },
    /// The database itself as the cache, for edge deployments with no Redis.
    Store,
}

/// The key that seals upstream credential secrets at rest, and the rotation
/// flow around it.
///
/// Rotation is two configured keys and a switch: `key` is what the instance
/// decrypts with today, `next` is what it re-seals to. With `rotate` set, the
/// binary performs the rotation at startup as one management operation and the
/// operator then promotes `next` to `key`. Leaving `key` unset stores secrets
/// in plaintext; that is a supported deployment, not an accident, and the
/// binary warns about it once at startup.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MasterKeyConfig {
    pub key: MasterKey,
    /// The key to re-seal to. Configured on its own it does nothing; the
    /// instance keeps using `key`.
    pub next: MasterKey,
    /// Perform the rotation at startup. Ignored without `next`.
    pub rotate: bool,
}

/// A 32-byte key, encoded. Two encodings because operators paste whichever
/// their secret manager emits, and guessing between them silently is worse
/// than naming it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum MasterKey {
    #[default]
    None,
    Hex(String),
    /// Standard or URL-safe alphabet, with or without padding.
    Base64(String),
}

impl MasterKey {
    /// Decode to the 32 bytes AES-GCM needs. A key of any other length is
    /// rejected rather than stretched: a truncated paste must not silently
    /// become a different, weaker key that still "works" until the day the
    /// original is restored.
    pub fn resolve(&self) -> Result<Option<[u8; 32]>, AppError> {
        let bytes = match self {
            Self::None => return Ok(None),
            Self::Hex(text) => crate::hex::decode(text.trim())
                .ok_or_else(|| AppError::invalid("master key is not valid hexadecimal"))?,
            Self::Base64(text) => decode_base64(text.trim())
                .ok_or_else(|| AppError::invalid("master key is not valid base64"))?,
        };
        let length = bytes.len();
        <[u8; 32]>::try_from(bytes.as_slice())
            .map(Some)
            .map_err(|_| AppError::invalid(format!("master key must be 32 bytes, got {length}")))
    }
}

/// Every base64 alphabet an operator might paste. Padding is optional in both.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    STANDARD
        .decode(text)
        .or_else(|_| STANDARD_NO_PAD.decode(text))
        .or_else(|_| URL_SAFE.decode(text))
        .or_else(|_| URL_SAFE_NO_PAD.decode(text))
        .ok()
}

impl MasterKeyConfig {
    /// The key in force now.
    pub fn resolve(&self) -> Result<Option<[u8; 32]>, AppError> {
        self.key.resolve()
    }

    /// The key a startup rotation re-seals to, if one was asked for. None
    /// means there is nothing to rotate, either because no next key was
    /// configured or because `rotate` is off.
    pub fn resolve_next(&self) -> Result<Option<[u8; 32]>, AppError> {
        if !self.rotate {
            return Ok(None);
        }
        self.next.resolve()
    }
}

/// Where publication and vocabulary bytes live. Unset means neither is served.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileStorageConfig {
    /// A local directory, resolved against `data_dir` when relative.
    Fs { root: String },
    S3 {
        bucket: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        region: Option<String>,
        /// Set for S3-compatible services that are not AWS.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access_key_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secret_access_key: Option<String>,
        /// Key prefix inside the bucket; empty uses the bucket root.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ConsoleConfig {
    pub enabled: bool,
    /// Serve the console from a directory instead of the embedded bundle.
    pub path: Option<String>,
}

impl Default for ConsoleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
        }
    }
}

/// Lifetimes of what the OAuth issuer mints. Access tokens are short because
/// they are bearer credentials with no revocation check on the hot path;
/// codes and device codes are short because they are one-shot handoffs.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OAuthIssuerConfig {
    pub access_ttl_secs: u64,
    pub refresh_ttl_secs: u64,
    pub code_ttl_secs: u64,
    pub device_ttl_secs: u64,
    /// Clients whose access tokens may perform *any* operation their
    /// permissions and granted scopes allow, rather than only the coding-agent
    /// baseline admission holds every other OAuth client to.
    ///
    /// Empty by default, which is the safe end: an access token is a
    /// credential a user handed to somebody else's program, so it starts able
    /// to list models, count tokens, generate, stream and compact only when
    /// their scopes were granted. An operator names a first-party CLI here once they accept that it
    /// speaks for the user across the whole API. See
    /// [`admission::permission`](crate::admission::permission).
    pub cli_client_ids: Vec<String>,
}

impl Default for OAuthIssuerConfig {
    fn default() -> Self {
        Self {
            access_ttl_secs: 3600,
            refresh_ttl_secs: THIRTY_DAYS_SECS,
            code_ttl_secs: 300,
            device_ttl_secs: 900,
            cli_client_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_document_is_the_default_configuration() {
        let config: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, AppConfig::default());
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8787);
        assert!(config.audit_enabled);
        assert_eq!(config.session_ttl_secs, 2_592_000);
        assert_eq!(config.oauth.access_ttl_secs, 3600);
        assert_eq!(config.oauth.refresh_ttl_secs, 2_592_000);
        assert_eq!(config.oauth.code_ttl_secs, 300);
        assert_eq!(config.oauth.device_ttl_secs, 900);
        assert!(config.oauth.cli_client_ids.is_empty());
        assert!(config.console.enabled);
        assert_eq!(
            config.store,
            StoreBackendConfig::Sqlite {
                path: "gproxy.db".into()
            }
        );
        assert_eq!(config.cache, CacheBackendConfig::Memory);
        assert_eq!(config.master_key.key, MasterKey::None);
        assert!(!config.master_key.rotate);
    }

    #[test]
    fn the_full_shape_round_trips() {
        let config = AppConfig {
            host: "0.0.0.0".into(),
            port: 8080,
            data_dir: Some("/var/lib/gproxy".into()),
            store: StoreBackendConfig::Url {
                dsn: "postgres://localhost/gproxy".into(),
            },
            cache: CacheBackendConfig::Redis {
                url: "redis://localhost".into(),
                namespace: Some("prod".into()),
            },
            master_key: MasterKeyConfig {
                key: MasterKey::Hex("aa".repeat(32)),
                next: MasterKey::Base64("Zm9v".into()),
                rotate: true,
            },
            file_storage: Some(FileStorageConfig::S3 {
                bucket: "gproxy".into(),
                region: Some("auto".into()),
                endpoint: None,
                access_key_id: None,
                secret_access_key: None,
                root: Some("files".into()),
            }),
            public_base_url: Some("https://gproxy.example.com".into()),
            cors_origins: vec!["https://console.example.com".into()],
            trusted_proxies: vec!["10.0.0.0/8".into()],
            console: ConsoleConfig {
                enabled: false,
                path: Some("/srv/console".into()),
            },
            audit_enabled: false,
            session_ttl_secs: 3600,
            oauth: OAuthIssuerConfig {
                access_ttl_secs: 60,
                refresh_ttl_secs: 120,
                code_ttl_secs: 30,
                device_ttl_secs: 45,
                cli_client_ids: vec!["codex".into()],
            },
        };
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<AppConfig>(&text).unwrap(), config);
    }

    #[test]
    fn a_partial_document_only_overrides_what_it_names() {
        let config: AppConfig = serde_json::from_str(r#"{"port": 9000}"#).unwrap();
        assert_eq!(config.port, 9000);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn an_unknown_key_is_a_typo_not_a_silent_no_op() {
        assert!(serde_json::from_str::<AppConfig>(r#"{"prot": 9000}"#).is_err());
    }

    #[test]
    fn backends_deserialize_from_their_tag() {
        let config: AppConfig = serde_json::from_str(
            r#"{"store": {"kind": "d1", "binding": "DB"}, "cache": {"kind": "store"}}"#,
        )
        .unwrap();
        assert_eq!(
            config.store,
            StoreBackendConfig::D1 {
                binding: "DB".into()
            }
        );
        assert_eq!(config.cache, CacheBackendConfig::Store);
    }

    #[test]
    fn no_master_key_means_plaintext_secrets() {
        assert_eq!(MasterKey::None.resolve().unwrap(), None);
    }

    #[test]
    fn a_hex_key_decodes_in_either_case() {
        let lower = MasterKey::Hex("ab".repeat(32)).resolve().unwrap().unwrap();
        let upper = MasterKey::Hex("AB".repeat(32)).resolve().unwrap().unwrap();
        assert_eq!(lower, [0xab_u8; 32]);
        assert_eq!(lower, upper);
    }

    #[test]
    fn a_base64_key_decodes_in_every_alphabet() {
        let raw = [0xfb_u8; 32];
        for text in [
            base64::engine::general_purpose::STANDARD.encode(raw),
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(raw),
            base64::engine::general_purpose::URL_SAFE.encode(raw),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw),
        ] {
            assert_eq!(MasterKey::Base64(text).resolve().unwrap().unwrap(), raw);
        }
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_key() {
        let key = MasterKey::Hex(format!("  {}\n", "cd".repeat(32)));
        assert_eq!(key.resolve().unwrap().unwrap(), [0xcd_u8; 32]);
    }

    #[test]
    fn a_key_of_the_wrong_length_is_rejected() {
        let short = MasterKey::Hex("ab".repeat(16)).resolve().unwrap_err();
        assert_eq!(short.status_code(), 400);
        assert!(short.to_string().contains("got 16"), "{short}");

        let long = MasterKey::Base64(base64::engine::general_purpose::STANDARD.encode([0_u8; 33]))
            .resolve()
            .unwrap_err();
        assert!(long.to_string().contains("got 33"), "{long}");
    }

    #[test]
    fn a_malformed_key_is_rejected_rather_than_partially_decoded() {
        assert!(MasterKey::Hex("zz".repeat(32)).resolve().is_err());
        assert!(MasterKey::Hex("abc".into()).resolve().is_err());
        assert!(MasterKey::Base64("not base64!!".into()).resolve().is_err());
    }

    #[test]
    fn a_next_key_is_only_resolved_when_rotation_was_asked_for() {
        let mut config = MasterKeyConfig {
            key: MasterKey::Hex("11".repeat(32)),
            next: MasterKey::Hex("22".repeat(32)),
            rotate: false,
        };
        assert_eq!(config.resolve().unwrap().unwrap(), [0x11_u8; 32]);
        assert_eq!(config.resolve_next().unwrap(), None);
        config.rotate = true;
        assert_eq!(config.resolve_next().unwrap().unwrap(), [0x22_u8; 32]);
    }
}
