//! Layering the four configuration sources into one [`AppConfig`].
//!
//! # The order, and why it is that order
//!
//! ```text
//! command line  >  environment  >  .env  >  --config file  >  defaults
//! ```
//!
//! The first three are already resolved by the time this module runs:
//! [`crate::env`] loaded `.env` without overriding the real environment, and
//! `clap` folded the environment into the same `Option` a flag would have
//! filled, preferring the flag. So what is left is one step — overlay whatever
//! the operator said onto whatever the file said, onto the defaults — and that
//! is [`settings`].
//!
//! Putting the file *under* the environment is the choice that matters. A file
//! is a deployment's checked-in intent; the environment is how one host,
//! one container or one operator deviates from it. If the file won, a
//! `GPROXY_PORT` in a compose file would silently do nothing.
//!
//! # Nothing here reads anything
//!
//! [`settings`] is a pure function of the parsed [`Cli`] and the file it names.
//! That is what makes the precedence testable without a process to observe: a
//! test builds a `Cli`, points it at a temporary file, and asserts on the
//! `AppConfig` that comes out.

use std::path::Path;

use gproxy_app::config::{
    AppConfig, CacheBackendConfig, ConsoleConfig, FileStorageConfig, MasterKey, MasterKeyConfig,
    StoreBackendConfig,
};

use crate::{
    Error, Result,
    cli::{self, Cli},
};

/// The default data directory, v3's. Relative, so a checkout and a container
/// both get a predictable `./data` rather than something under `$HOME` that
/// depends on who started the process.
pub const DEFAULT_DATA_DIR: &str = "data";

/// Everything one invocation needs, after layering.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// What both hosts understand. The only thing that travels into
    /// [`gproxy_app::App`].
    pub config: AppConfig,
    /// The first-start administrator. Not part of `AppConfig` because it is not
    /// configuration of a running instance — it is a one-time instruction.
    pub admin: AdminOptions,
    /// The tracing subscriber, which has to be up before anything else can
    /// report a failure.
    pub telemetry: TelemetryOptions,
    /// A stable name for this process, for continuations that pin an upstream
    /// connection to the instance holding it.
    pub instance_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminOptions {
    pub user: String,
    /// Unset means "generate one and print it once".
    pub password: Option<String>,
    /// Unset means "mint a random key". Set means mint exactly this text, which
    /// is what lets a deployment provision a key it already has in a secret
    /// manager.
    pub api_key: Option<String>,
}

pub const DEFAULT_ADMIN_USER: &str = "admin";

impl Default for AdminOptions {
    fn default() -> Self {
        Self {
            user: DEFAULT_ADMIN_USER.to_owned(),
            password: None,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryOptions {
    pub format: LogFormat,
    pub filter: String,
}

impl Default for TelemetryOptions {
    fn default() -> Self {
        Self {
            format: LogFormat::Text,
            filter: "info".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

/// Layer the parsed command line over the file it names, over the defaults.
pub fn settings(cli: &Cli) -> Result<Settings> {
    let options = &cli.options;
    let file = match options.config.as_deref() {
        Some(path) => Some(read_file(path)?),
        None => None,
    };
    layer(options, file)
}

/// The pure half, for tests and for a host that parsed its own arguments.
pub fn layer(options: &cli::Options, file: Option<AppConfig>) -> Result<Settings> {
    let mut config = file.unwrap_or_default();

    if let Some(host) = text(&options.host) {
        config.host = host;
    }
    if let Some(port) = text(&options.port) {
        config.port = port
            .parse()
            .map_err(|_| Error::config(source(cli::PORT), format!("`{port}` is not a port")))?;
    }
    if let Some(data_dir) = text(&options.data_dir) {
        config.data_dir = Some(data_dir);
    }
    // Not a flag override: the default belongs here rather than in `AppConfig`,
    // because the edge host has no filesystem to resolve it against.
    if config.data_dir.is_none() {
        config.data_dir = Some(DEFAULT_DATA_DIR.to_owned());
    }

    if let Some(store) = store_backend(options)? {
        config.store = store;
    }
    if let Some(url) = text(&options.redis_url) {
        config.cache = CacheBackendConfig::Redis {
            url,
            namespace: None,
        };
    }
    if let Some(root) = text(&options.file_storage_dir) {
        config.file_storage = Some(FileStorageConfig::Fs { root });
    }

    config.master_key = master_key(options, &config.master_key)?;

    if let Some(url) = text(&options.public_base_url) {
        config.public_base_url = Some(url);
    }
    if let Some(origins) = text(&options.cors_origins) {
        config.cors_origins = list(&origins);
    }
    if let Some(proxies) = text(&options.trusted_proxies) {
        config.trusted_proxies = list(&proxies);
    }

    config.console = console(options, &config.console)?;
    if let Some(value) = options.audit_enabled.as_deref() {
        config.audit_enabled = boolean(value, cli::AUDIT_ENABLED)?;
    }

    Ok(Settings {
        config,
        admin: AdminOptions {
            user: text(&options.admin_user).unwrap_or_else(|| DEFAULT_ADMIN_USER.to_owned()),
            // Not trimmed, and not rejected when blank: the password families
            // in `gproxy-app` own that policy, and a second opinion here could
            // only disagree with them.
            password: options.admin_password.clone().filter(|v| !v.is_empty()),
            api_key: text(&options.admin_api_key),
        },
        telemetry: telemetry(options)?,
        instance_id: text(&options.instance_id),
    })
}

/// The store backend the operator named, or `None` when they named neither a
/// backend nor a connection string and the file's choice stands.
fn store_backend(options: &cli::Options) -> Result<Option<StoreBackendConfig>> {
    let dsn = text(&options.dsn);
    let named = text(&options.persistence).map(|value| value.to_ascii_lowercase());
    let backend = match (named.as_deref(), dsn.as_deref()) {
        (None, None) => return Ok(None),
        // A connection string on its own names its own backend. Guessing is
        // safe here and only here, because the scheme is unambiguous.
        (None, Some(dsn)) => match dsn.split_once("://") {
            Some(("postgres" | "postgresql", _)) => "postgres",
            Some(("mysql", _)) => "mysql",
            Some(("sqlite", _)) | None => "sqlite",
            Some((scheme, _)) => {
                return Err(Error::config(
                    source(cli::DSN),
                    format!(
                        "`{scheme}://` names no known backend; set {} explicitly",
                        cli::PERSISTENCE
                    ),
                ));
            }
        },
        (Some(named), _) => named,
    };

    let backend = match backend {
        // `db` is what v2 called it, and v3 kept the alias.
        "sqlite" | "db" => StoreBackendConfig::Sqlite {
            path: match dsn {
                // A v2/v3 deployment may still carry a `sqlite://…` DSN. The
                // file path inside it is what this build needs.
                Some(dsn) => sqlite_path(&dsn)?,
                None => "gproxy.db".to_owned(),
            },
        },
        "postgres" | "postgresql" | "mysql" => StoreBackendConfig::Url {
            dsn: dsn.ok_or_else(|| {
                Error::config(
                    source(cli::DSN),
                    format!("a `{backend}` backend needs a connection string"),
                )
            })?,
        },
        other => {
            return Err(Error::config(
                source(cli::PERSISTENCE),
                format!("`{other}` is not a backend; expected `sqlite`, `postgres` or `mysql`"),
            ));
        }
    };
    Ok(Some(backend))
}

/// The file path inside a SQLite DSN, or the DSN itself when it is already a
/// path.
///
/// `:memory:` is refused rather than accepted: an instance whose database
/// disappears when the process does would start, serve, bootstrap an
/// administrator and lose all of it, which is never what an operator setting a
/// DSN meant.
fn sqlite_path(dsn: &str) -> Result<String> {
    let rest = dsn.strip_prefix("sqlite://").unwrap_or(dsn);
    let path = rest.split_once('?').map_or(rest, |(path, _)| path);
    if path.is_empty() || path.contains(":memory:") {
        return Err(Error::config(
            source(cli::DSN),
            "a SQLite instance needs a file; `:memory:` would not survive the process",
        ));
    }
    Ok(path.to_owned())
}

/// The master key, its rotation target and whether rotation is armed.
///
/// The file's value is the fallback for each of the three independently, so a
/// deployment can keep the key in a file and arm the rotation from the
/// environment for one restart.
fn master_key(options: &cli::Options, file: &MasterKeyConfig) -> Result<MasterKeyConfig> {
    let key = match text(&options.master_key) {
        Some(value) => encoded(&value, cli::MASTER_KEY)?,
        None => file.key.clone(),
    };
    let next = match text(&options.master_key_next) {
        Some(value) => encoded(&value, cli::MASTER_KEY_NEXT)?,
        None => file.next.clone(),
    };
    let rotate = match options.master_rotate_requested() {
        Some(value) => boolean(value, cli::MASTER_KEY_ROTATE)?,
        None => file.rotate,
    };
    Ok(MasterKeyConfig { key, next, rotate })
}

/// A key as an operator pastes it: 64 hexadecimal characters, or base64 in any
/// alphabet.
///
/// The encoding is sniffed rather than configured, and only one shape counts as
/// hex: exactly 64 hex digits. That is not a heuristic that can be wrong in a
/// dangerous direction — a 32-byte base64 key is 43 or 44 characters and can
/// never be 64 hex digits — and it saves every deployment from a
/// `GPROXY_MASTER_KEY_ENCODING` variable nobody would get right the first time.
/// The decode is then verified immediately, so a bad paste fails at startup
/// rather than on the first credential.
fn encoded(value: &str, name: &str) -> Result<MasterKey> {
    let trimmed = value.trim();
    let key = if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        MasterKey::Hex(trimmed.to_owned())
    } else {
        MasterKey::Base64(trimmed.to_owned())
    };
    key.resolve()
        .map_err(|error| Error::config(source(name), error))?;
    Ok(key)
}

fn console(options: &cli::Options, file: &ConsoleConfig) -> Result<ConsoleConfig> {
    let enabled = match options.console_requested() {
        Some(value) => boolean(value, cli::CONSOLE)?,
        None => file.enabled,
    };
    Ok(ConsoleConfig {
        enabled,
        path: text(&options.console_path).or_else(|| file.path.clone()),
    })
}

fn telemetry(options: &cli::Options) -> Result<TelemetryOptions> {
    let format = match text(&options.log_format)
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        None | Some("text") => LogFormat::Text,
        Some("json") => LogFormat::Json,
        Some(other) => {
            return Err(Error::config(
                source(cli::LOG_FORMAT),
                format!("`{other}` is not a log format; expected `text` or `json`"),
            ));
        }
    };
    // `RUST_LOG` is read here and nowhere else, and it is the weakest source:
    // it is the ambient Rust convention, so a deployment that sets the explicit
    // variable must not have it quietly overruled by a shell export.
    let filter = text(&options.log_filter)
        .or_else(|| std::env::var("RUST_LOG").ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "info".to_owned());
    Ok(TelemetryOptions { format, filter })
}

/// v3's boolean vocabulary, and nothing outside it.
///
/// A misspelling is an error rather than a `false`, because the two values this
/// gates — rotating every secret in the database, and serving the console —
/// are both things an operator would not notice the absence of until it
/// mattered.
pub(crate) fn boolean(value: &str, name: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "" | "0" | "false" | "no" | "off" => Ok(false),
        other => Err(Error::config(
            source(name),
            format!(
                "`{other}` is not a boolean; expected one of `1`, `true`, `yes`, `on`, `0`, `false`, `no`, `off`"
            ),
        )),
    }
}

/// A comma-separated list, with blanks dropped. Empty stays empty rather than
/// becoming a list with one empty entry, which would be an origin that matches
/// nothing and looks configured.
fn list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A value the operator actually supplied. An empty string counts as silence:
/// an unset variable and one set to `""` are the same intent in every
/// deployment tool that produces them.
pub(crate) fn text(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// `--port / GPROXY_PORT`, the pair an error message names.
pub(crate) fn source(env: &str) -> String {
    let flag = env
        .strip_prefix("GPROXY_")
        .unwrap_or(env)
        .to_ascii_lowercase()
        .replace('_', "-");
    format!("--{flag} / {env}")
}

fn read_file(path: &Path) -> Result<AppConfig> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| Error::io(format!("reading {}", path.display()), error))?;
    toml::from_str(&text)
        .map_err(|error| Error::config(source(cli::CONFIG), format!("{}: {error}", path.display())))
}

impl cli::Options {
    /// Whether rotation was mentioned at all, as text. Separate from the parse
    /// so the layering can tell "said nothing" from "said no".
    pub fn master_rotate_requested(&self) -> Option<&str> {
        self.master_key_rotate.as_deref()
    }

    pub fn console_requested(&self) -> Option<&str> {
        self.console.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> cli::Options {
        cli::Options::default()
    }

    #[test]
    fn audit_switch_defaults_file_override_and_invalid_value() {
        assert!(
            layer(&cli::Options::default(), None)
                .unwrap()
                .config
                .audit_enabled
        );
        let file: AppConfig = toml::from_str("audit_enabled = false").unwrap();
        assert!(
            !layer(&cli::Options::default(), Some(file.clone()))
                .unwrap()
                .config
                .audit_enabled
        );
        let options = cli::Options {
            audit_enabled: Some("true".into()),
            ..Default::default()
        };
        assert!(layer(&options, Some(file)).unwrap().config.audit_enabled);
        let options = cli::Options {
            audit_enabled: Some("false".into()),
            ..Default::default()
        };
        assert!(!layer(&options, None).unwrap().config.audit_enabled);
        let options = cli::Options {
            audit_enabled: Some("typo".into()),
            ..Default::default()
        };
        assert!(layer(&options, None).is_err());
    }

    #[test]
    fn nothing_said_is_the_default_configuration_plus_a_data_directory() {
        let settings = layer(&options(), None).unwrap();
        assert_eq!(settings.config.host, "127.0.0.1");
        assert_eq!(settings.config.port, 8787);
        assert_eq!(settings.config.data_dir.as_deref(), Some(DEFAULT_DATA_DIR));
        assert_eq!(settings.admin.user, DEFAULT_ADMIN_USER);
        assert_eq!(settings.telemetry.format, LogFormat::Text);
        assert!(settings.config.console.enabled);
    }

    #[test]
    fn a_flag_beats_the_file() {
        let file = AppConfig {
            port: 1000,
            host: "10.0.0.1".into(),
            ..AppConfig::default()
        };
        let settings = layer(
            &cli::Options {
                port: Some("9000".into()),
                ..options()
            },
            Some(file),
        )
        .unwrap();
        assert_eq!(settings.config.port, 9000);
        // And what the flag did not mention is still the file's.
        assert_eq!(settings.config.host, "10.0.0.1");
    }

    #[test]
    fn a_bad_port_names_the_flag_and_the_variable() {
        let error = layer(
            &cli::Options {
                port: Some("seventy".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--port / GPROXY_PORT: `seventy` is not a port"
        );
    }

    #[test]
    fn an_empty_value_is_silence_not_an_override() {
        let file = AppConfig {
            host: "10.0.0.1".into(),
            ..AppConfig::default()
        };
        let settings = layer(
            &cli::Options {
                host: Some("   ".into()),
                ..options()
            },
            Some(file),
        )
        .unwrap();
        assert_eq!(settings.config.host, "10.0.0.1");
    }

    #[test]
    fn a_dsn_alone_names_its_own_backend() {
        for (dsn, expected) in [
            (
                "postgres://localhost/gproxy",
                StoreBackendConfig::Url {
                    dsn: "postgres://localhost/gproxy".into(),
                },
            ),
            (
                "mysql://localhost/gproxy",
                StoreBackendConfig::Url {
                    dsn: "mysql://localhost/gproxy".into(),
                },
            ),
            (
                "sqlite:///var/lib/gproxy/gproxy.db?mode=rwc",
                StoreBackendConfig::Sqlite {
                    path: "/var/lib/gproxy/gproxy.db".into(),
                },
            ),
            (
                "instance.db",
                StoreBackendConfig::Sqlite {
                    path: "instance.db".into(),
                },
            ),
        ] {
            let settings = layer(
                &cli::Options {
                    dsn: Some(dsn.into()),
                    ..options()
                },
                None,
            )
            .unwrap();
            assert_eq!(settings.config.store, expected, "{dsn}");
        }
    }

    #[test]
    fn a_memory_dsn_is_refused_rather_than_silently_ephemeral() {
        let error = layer(
            &cli::Options {
                dsn: Some("sqlite::memory:".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("would not survive"), "{error}");
    }

    #[test]
    fn a_server_backend_without_a_connection_string_is_refused() {
        let error = layer(
            &cli::Options {
                persistence: Some("postgres".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("connection string"), "{error}");
    }

    #[test]
    fn an_unknown_backend_lists_the_known_ones() {
        let error = layer(
            &cli::Options {
                persistence: Some("cassandra".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("`sqlite`"), "{error}");
    }

    #[test]
    fn v2s_db_alias_still_selects_sqlite() {
        let settings = layer(
            &cli::Options {
                persistence: Some("DB".into()),
                ..options()
            },
            None,
        )
        .unwrap();
        assert_eq!(
            settings.config.store,
            StoreBackendConfig::Sqlite {
                path: "gproxy.db".into()
            }
        );
    }

    #[test]
    fn a_hex_key_and_a_base64_key_decode_to_the_same_bytes() {
        use base64::Engine;
        let raw = [0x5a_u8; 32];
        let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);

        for encoding in [hex, b64] {
            let settings = layer(
                &cli::Options {
                    master_key: Some(encoding.clone()),
                    ..options()
                },
                None,
            )
            .unwrap();
            assert_eq!(
                settings.config.master_key.resolve().unwrap(),
                Some(raw),
                "{encoding}"
            );
        }
    }

    #[test]
    fn a_key_of_the_wrong_length_is_refused_at_startup() {
        let error = layer(
            &cli::Options {
                master_key: Some("ab".repeat(8)),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("--master-key / GPROXY_MASTER_KEY")
        );
        assert!(error.to_string().contains("32 bytes"), "{error}");
    }

    #[test]
    fn rotation_is_armed_only_by_the_vocabulary() {
        for (value, expected) in [
            ("true", true),
            ("1", true),
            ("ON", true),
            ("false", false),
            ("no", false),
        ] {
            let settings = layer(
                &cli::Options {
                    master_key_rotate: Some(value.into()),
                    ..options()
                },
                None,
            )
            .unwrap();
            assert_eq!(settings.config.master_key.rotate, expected, "{value}");
        }
        let error = layer(
            &cli::Options {
                master_key_rotate: Some("maybe".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not a boolean"), "{error}");
    }

    #[test]
    fn saying_nothing_about_rotation_leaves_the_files_answer() {
        let file = AppConfig {
            master_key: MasterKeyConfig {
                key: MasterKey::Hex("11".repeat(32)),
                next: MasterKey::Hex("22".repeat(32)),
                rotate: true,
            },
            ..AppConfig::default()
        };
        let settings = layer(&options(), Some(file)).unwrap();
        assert!(settings.config.master_key.rotate);
        assert_eq!(
            settings.config.master_key.resolve_next().unwrap(),
            Some([0x22_u8; 32])
        );
    }

    #[test]
    fn lists_are_comma_separated_and_drop_blanks() {
        let settings = layer(
            &cli::Options {
                cors_origins: Some("https://a.example, ,https://b.example ".into()),
                trusted_proxies: Some("10.0.0.1".into()),
                ..options()
            },
            None,
        )
        .unwrap();
        assert_eq!(
            settings.config.cors_origins,
            vec![
                "https://a.example".to_owned(),
                "https://b.example".to_owned()
            ]
        );
        assert_eq!(settings.config.trusted_proxies, vec!["10.0.0.1".to_owned()]);
    }

    #[test]
    fn the_console_switch_and_its_directory_are_independent() {
        let settings = layer(
            &cli::Options {
                console: Some("false".into()),
                console_path: Some("/srv/console".into()),
                ..options()
            },
            None,
        )
        .unwrap();
        assert!(!settings.config.console.enabled);
        assert_eq!(
            settings.config.console.path.as_deref(),
            Some("/srv/console")
        );
    }

    #[test]
    fn redis_and_local_file_storage_replace_the_defaults() {
        let settings = layer(
            &cli::Options {
                redis_url: Some("redis://localhost".into()),
                file_storage_dir: Some("files".into()),
                ..options()
            },
            None,
        )
        .unwrap();
        assert_eq!(
            settings.config.cache,
            CacheBackendConfig::Redis {
                url: "redis://localhost".into(),
                namespace: None
            }
        );
        assert_eq!(
            settings.config.file_storage,
            Some(FileStorageConfig::Fs {
                root: "files".into()
            })
        );
    }

    #[test]
    fn an_unknown_log_format_is_refused() {
        let error = layer(
            &cli::Options {
                log_format: Some("logfmt".into()),
                ..options()
            },
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("not a log format"), "{error}");
    }
}
