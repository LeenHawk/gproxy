//! What a Worker is configured with, and how that becomes an [`AppConfig`].
//!
//! # One vocabulary, two hosts
//!
//! The native binary layers `clap` → environment → `.env` → `gproxy.toml` into
//! an [`AppConfig`]. A Worker has none of those: it has *bindings*, which are
//! strings the platform hands the isolate, and there is no file to read and no
//! command line to parse. So the layering here is deliberately two levels deep
//! and no more:
//!
//! 1. **`GPROXY_CONFIG`**, one JSON document in exactly the [`AppConfig`]
//!    shape. Every field is optional; the whole variable is optional.
//! 2. **the named secrets**, each one overriding the corresponding field.
//!
//! The split is not stylistic. Cloudflare stores `[vars]` in `wrangler.toml`,
//! in the repository, in plain text; `wrangler secret put` stores secrets
//! encrypted and never shows them again. A master key and an S3 credential
//! belong in the second place, so they cannot be written into the first — and
//! the only way to make that true is to let a secret override the document
//! rather than live in it.
//!
//! # What a Worker ignores
//!
//! `host`, `port` and `data_dir` describe a process with a socket and a
//! filesystem. An isolate has neither, and the runtime chooses the address.
//! They are accepted (the type is shared) and never read.

use gproxy_app::config::{
    AppConfig, CacheBackendConfig, FileStorageConfig, MasterKey, StoreBackendConfig,
};

/// The binding names this host reads. Kept as constants because they are also
/// the README's table and `wrangler.toml.example`'s keys, and three copies of
/// a string is how those drift apart.
pub mod binding {
    /// The whole configuration document, as JSON.
    pub const CONFIG: &str = "GPROXY_CONFIG";
    /// 32 bytes, as 64 hex digits or as base64. Absent means secrets are
    /// stored in plaintext, which the host warns about once.
    pub const MASTER_KEY: &str = "GPROXY_MASTER_KEY";
    /// The libSQL/Turso bearer token, when the store is libSQL.
    pub const LIBSQL_TOKEN: &str = "GPROXY_LIBSQL_TOKEN";
    /// S3/R2 credentials, when file storage is configured.
    pub const S3_ACCESS_KEY_ID: &str = "GPROXY_S3_ACCESS_KEY_ID";
    pub const S3_SECRET_ACCESS_KEY: &str = "GPROXY_S3_SECRET_ACCESS_KEY";

    /// First-run identity bindings; these are not part of AppConfig.
    pub const ADMIN_USER: &str = "GPROXY_ADMIN_USER";
    pub const ADMIN_PASSWORD: &str = "GPROXY_ADMIN_PASSWORD";

    /// Shared AppConfig secrets. The bootstrap password is read separately.
    pub const SECRETS: [&str; 4] = [
        MASTER_KEY,
        LIBSQL_TOKEN,
        S3_ACCESS_KEY_ID,
        S3_SECRET_ACCESS_KEY,
    ];
}

/// The secrets, read out of the environment by whoever has one.
///
/// A separate type so [`resolve`] is a pure function of two strings-in,
/// config-out and can be tested on the host target, where there is no `Env` to
/// read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Secrets {
    pub master_key: Option<String>,
    pub libsql_token: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
}

/// Why a configuration could not be made. Rendered into the Worker's own error
/// at the call site; kept as a string because nothing branches on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

fn invalid(message: impl Into<String>) -> ConfigError {
    ConfigError(message.into())
}

/// The document, before the secrets are laid over it.
///
/// There is no shadow type: the text is deserialized into [`AppConfig`]
/// itself, because a second shape would be a second vocabulary and the whole
/// point of `GPROXY_CONFIG` is that it is the one the native host already
/// reads out of `gproxy.toml`.
fn default_store() -> serde_json::Value {
    serde_json::json!({ "kind": "d1", "binding": DEFAULT_D1_BINDING })
}

fn default_cache() -> serde_json::Value {
    serde_json::json!({ "kind": "store" })
}

/// Turn the two inputs into the configuration this isolate will run on.
///
/// `document` is `GPROXY_CONFIG`'s text, or `None` when the binding is absent
/// — which is a supported deployment, not an error: a Worker with a `DB`
/// binding and nothing else gets D1, the database-backed cache and no file
/// storage, which is the smallest thing that serves traffic.
///
/// A document that names *some* fields gets the edge defaults for the two it
/// did not name, rather than [`AppConfig`]'s own. That matters because
/// `AppConfig::default()` describes the native binary — a SQLite file and an
/// in-process cache — and a Worker can open neither. The defaults are filled
/// into the JSON before it is deserialized, so there is still one type and one
/// deserialization; what would not work is deciding afterwards, where an
/// absent `store` and a deliberate `{"kind":"sqlite"}` look the same.
pub fn resolve(document: Option<&str>, secrets: &Secrets) -> Result<AppConfig, ConfigError> {
    let mut config = match document.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => {
            let mut value: serde_json::Value = serde_json::from_str(text)
                .map_err(|error| invalid(format!("{}: {error}", binding::CONFIG)))?;
            let Some(object) = value.as_object_mut() else {
                return Err(invalid(format!(
                    "{} must be a JSON object",
                    binding::CONFIG
                )));
            };
            object.entry("store").or_insert_with(default_store);
            object.entry("cache").or_insert_with(default_cache);
            serde_json::from_value::<AppConfig>(value)
                .map_err(|error| invalid(format!("{}: {error}", binding::CONFIG)))?
        }
        None => AppConfig {
            store: StoreBackendConfig::D1 {
                binding: DEFAULT_D1_BINDING.to_owned(),
            },
            cache: CacheBackendConfig::Store,
            ..AppConfig::default()
        },
    };

    if let Some(key) = text(&secrets.master_key) {
        config.master_key.key = encoded(key, binding::MASTER_KEY)?;
    }
    if let Some(token) = text(&secrets.libsql_token) {
        match &mut config.store {
            StoreBackendConfig::Libsql { token: slot, .. } => *slot = Some(token.to_owned()),
            _ => {
                return Err(invalid(format!(
                    "{} is set but the store is not libSQL",
                    binding::LIBSQL_TOKEN
                )));
            }
        }
    }
    let s3_id = text(&secrets.s3_access_key_id);
    let s3_secret = text(&secrets.s3_secret_access_key);
    if s3_id.is_some() || s3_secret.is_some() {
        match &mut config.file_storage {
            Some(FileStorageConfig::S3 {
                access_key_id,
                secret_access_key,
                ..
            }) => {
                if let Some(value) = s3_id {
                    *access_key_id = Some(value.to_owned());
                }
                if let Some(value) = s3_secret {
                    *secret_access_key = Some(value.to_owned());
                }
            }
            _ => {
                return Err(invalid(format!(
                    "{} / {} are set but no S3 file storage is configured",
                    binding::S3_ACCESS_KEY_ID,
                    binding::S3_SECRET_ACCESS_KEY
                )));
            }
        }
    }

    // The cache is the one place a Worker cannot take the shared default.
    // `Memory` is per isolate, and an isolate is created and destroyed around
    // a request: a rate-limit counter kept there counts to one forever, and a
    // login session written there is gone before the redirect comes back. The
    // refusal is here rather than at the first wrong answer.
    if config.cache == CacheBackendConfig::Memory {
        return Err(invalid(
            "an in-memory cache cannot be shared between Worker isolates; configure \
             `{\"cache\":{\"kind\":\"store\"}}` or a Redis URL",
        ));
    }
    if matches!(config.store, StoreBackendConfig::Sqlite { .. })
        || matches!(config.store, StoreBackendConfig::Url { .. })
    {
        return Err(invalid(
            "a Worker cannot open a local file or a TCP database; configure \
             `{\"store\":{\"kind\":\"d1\",\"binding\":\"DB\"}}` or a libSQL URL",
        ));
    }
    Ok(config)
}

/// The binding name a Worker gets when `GPROXY_CONFIG` says nothing. `DB` is
/// what `wrangler d1 create` suggests and what the example file uses.
pub const DEFAULT_D1_BINDING: &str = "DB";

fn text(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// Sniff the master key's encoding, on exactly the rule the native binary uses
/// (`crates/gproxy/src/config.rs`): 64 hex digits is hex, anything else is
/// base64. A 32-byte base64 key is 43 or 44 characters, so the two can never
/// be confused, and an operator never has to name an encoding.
///
/// The decode is verified here so a bad paste fails while the isolate is
/// assembling rather than on the first credential it tries to open.
fn encoded(value: &str, name: &str) -> Result<MasterKey, ConfigError> {
    let key = if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        MasterKey::Hex(value.to_owned())
    } else {
        MasterKey::Base64(value.to_owned())
    };
    key.resolve()
        .map_err(|error| invalid(format!("{name}: {error}")))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn secrets() -> Secrets {
        Secrets::default()
    }

    #[test]
    fn no_document_is_a_d1_worker_with_the_database_backed_cache() {
        let config = resolve(None, &secrets()).unwrap();
        assert_eq!(
            config.store,
            StoreBackendConfig::D1 {
                binding: "DB".into()
            }
        );
        assert_eq!(config.cache, CacheBackendConfig::Store);
        assert!(config.file_storage.is_none());
    }

    #[test]
    fn an_empty_document_is_the_same_as_none() {
        assert_eq!(
            resolve(Some("   "), &secrets()).unwrap(),
            resolve(None, &secrets()).unwrap()
        );
    }

    #[test]
    fn the_document_is_the_shared_app_config_shape() {
        let config = resolve(
            Some(
                r#"{
                    "store": {"kind": "libsql", "url": "libsql://db.turso.io"},
                    "cache": {"kind": "store"},
                    "public_base_url": "https://gproxy.example.com",
                    "cors_origins": ["https://console.example.com"]
                }"#,
            ),
            &secrets(),
        )
        .unwrap();
        assert_eq!(
            config.store,
            StoreBackendConfig::Libsql {
                url: "libsql://db.turso.io".into(),
                token: None,
            }
        );
        assert_eq!(
            config.public_base_url.as_deref(),
            Some("https://gproxy.example.com")
        );
        assert_eq!(config.cors_origins, vec!["https://console.example.com"]);
    }

    #[test]
    fn a_document_that_names_neither_store_nor_cache_still_gets_the_edge_defaults() {
        let config = resolve(
            Some(r#"{"public_base_url":"https://x.example"}"#),
            &secrets(),
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
    fn a_document_that_is_not_an_object_names_the_binding() {
        let error = resolve(Some("[]"), &secrets()).unwrap_err();
        assert!(error.0.contains("GPROXY_CONFIG"), "{error}");
    }

    #[test]
    fn a_malformed_document_names_the_binding() {
        let error = resolve(Some("{not json"), &secrets()).unwrap_err();
        assert!(error.0.contains("GPROXY_CONFIG"), "{error}");
    }

    #[test]
    fn a_sixty_four_digit_key_is_hex_and_anything_else_is_base64() {
        let hex = resolve(
            None,
            &Secrets {
                master_key: Some(KEY_HEX.into()),
                ..secrets()
            },
        )
        .unwrap();
        assert_eq!(hex.master_key.key, MasterKey::Hex(KEY_HEX.into()));

        let base64 = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
        let sniffed = resolve(
            None,
            &Secrets {
                master_key: Some(base64.into()),
                ..secrets()
            },
        )
        .unwrap();
        assert_eq!(sniffed.master_key.key, MasterKey::Base64(base64.into()));
    }

    #[test]
    fn a_key_that_is_not_thirty_two_bytes_fails_while_assembling() {
        let error = resolve(
            None,
            &Secrets {
                master_key: Some("abcd".into()),
                ..secrets()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("GPROXY_MASTER_KEY"), "{error}");
    }

    #[test]
    fn an_empty_secret_is_not_a_secret() {
        let config = resolve(
            None,
            &Secrets {
                master_key: Some("   ".into()),
                ..secrets()
            },
        )
        .unwrap();
        assert_eq!(config.master_key.key, MasterKey::None);
    }

    #[test]
    fn the_libsql_token_lands_on_the_store_it_belongs_to() {
        let config = resolve(
            Some(r#"{"store":{"kind":"libsql","url":"libsql://db.turso.io"},"cache":{"kind":"store"}}"#),
            &Secrets {
                libsql_token: Some("tok".into()),
                ..secrets()
            },
        )
        .unwrap();
        assert_eq!(
            config.store,
            StoreBackendConfig::Libsql {
                url: "libsql://db.turso.io".into(),
                token: Some("tok".into()),
            }
        );
    }

    #[test]
    fn a_libsql_token_without_a_libsql_store_is_a_misconfiguration() {
        let error = resolve(
            None,
            &Secrets {
                libsql_token: Some("tok".into()),
                ..secrets()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("not libSQL"), "{error}");
    }

    #[test]
    fn the_s3_credentials_land_on_the_configured_bucket() {
        let config = resolve(
            Some(r#"{"cache":{"kind":"store"},"file_storage":{"kind":"s3","bucket":"gproxy"}}"#),
            &Secrets {
                s3_access_key_id: Some("id".into()),
                s3_secret_access_key: Some("secret".into()),
                ..secrets()
            },
        )
        .unwrap();
        let Some(FileStorageConfig::S3 {
            bucket,
            access_key_id,
            secret_access_key,
            ..
        }) = config.file_storage
        else {
            panic!("expected S3 storage");
        };
        assert_eq!(bucket, "gproxy");
        assert_eq!(access_key_id.as_deref(), Some("id"));
        assert_eq!(secret_access_key.as_deref(), Some("secret"));
    }

    #[test]
    fn s3_credentials_without_a_bucket_are_a_misconfiguration() {
        let error = resolve(
            None,
            &Secrets {
                s3_access_key_id: Some("id".into()),
                ..secrets()
            },
        )
        .unwrap_err();
        assert!(error.0.contains("no S3 file storage"), "{error}");
    }

    #[test]
    fn a_per_isolate_cache_is_refused_rather_than_silently_wrong() {
        let error = resolve(Some(r#"{"cache":{"kind":"memory"}}"#), &secrets()).unwrap_err();
        assert!(error.0.contains("isolates"), "{error}");
    }

    #[test]
    fn a_store_a_worker_cannot_open_is_refused_while_assembling() {
        for store in [
            r#"{"kind":"sqlite","path":"gproxy.db"}"#,
            r#"{"kind":"url","dsn":"postgres://localhost/gproxy"}"#,
        ] {
            let document = format!(r#"{{"store":{store},"cache":{{"kind":"store"}}}}"#);
            let error = resolve(Some(&document), &secrets()).unwrap_err();
            assert!(error.0.contains("Worker cannot open"), "{error}");
        }
    }

    #[test]
    fn redis_is_a_cache_a_worker_may_name() {
        let config = resolve(
            Some(r#"{"cache":{"kind":"redis","url":"rediss://cache.example.com"}}"#),
            &secrets(),
        )
        .unwrap();
        assert!(matches!(config.cache, CacheBackendConfig::Redis { .. }));
    }
}
