use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{CacheConfig, Config, LogFormat, MasterKeyConfig, NativeOptions, invalid};
use crate::ConfigError;

const HOST: &str = "GPROXY_HOST";
const PORT: &str = "GPROXY_PORT";
const DATA_DIR: &str = "GPROXY_DATA_DIR";
const PERSISTENCE: &str = "GPROXY_PERSISTENCE";
const DSN: &str = "GPROXY_DSN";
const LIBSQL_URL: &str = "GPROXY_LIBSQL_URL";
const LIBSQL_AUTH_TOKEN: &str = "GPROXY_LIBSQL_AUTH_TOKEN";
const REDIS_URL: &str = "GPROXY_REDIS_URL";
const UPSTASH_URL: &str = "UPSTASH_URL";
const UPSTASH_TOKEN: &str = "UPSTASH_TOKEN";
const MASTER_KEY: &str = "GPROXY_MASTER_KEY";
const MASTER_KEY_NEXT: &str = "GPROXY_MASTER_KEY_NEXT";
const MASTER_KEY_ROTATE: &str = "GPROXY_MASTER_KEY_ROTATE";
const UPSTREAM_PROXY_URL: &str = "GPROXY_UPSTREAM_PROXY_URL";
const TRUSTED_PROXIES: &str = "GPROXY_TRUSTED_PROXIES";
const CORS_ORIGINS: &str = "GPROXY_CORS_ORIGINS";
const MAX_ATTEMPTS: &str = "GPROXY_MAX_ATTEMPTS";
const MAX_IN_FLIGHT: &str = "GPROXY_MAX_IN_FLIGHT";
const FILE_UPLOAD_MAX_IN_FLIGHT: &str = "GPROXY_FILE_UPLOAD_MAX_IN_FLIGHT";
const INSTANCE_ID: &str = "GPROXY_INSTANCE_ID";
const LOG_FORMAT: &str = "GPROXY_LOG_FORMAT";
const ADMIN_USER: &str = "GPROXY_ADMIN_USER";
const ADMIN_PASSWORD: &str = "GPROXY_ADMIN_PASSWORD";
const BOOTSTRAP_ADMIN_API_KEY: &str = "GPROXY_BOOTSTRAP_ADMIN_API_KEY";
const BOOTSTRAP_CHANNELS: &str = "GPROXY_BOOTSTRAP_CHANNELS";

/// The whole configuration surface, declared once. `--help` is generated
/// from this, so the flag list and the environment list cannot drift apart.
/// Every value is optional here because `.env` and the defaults are layered
/// underneath: command line, then real environment, then `.env`, then the
/// default.
///
/// Names match v2's wherever the meaning matches, so an operator moving a
/// deployment across does not have to relearn the surface.
#[derive(Debug, Default, clap::Parser)]
#[command(
    name = "gproxy",
    version,
    about = "GPROXY — one gateway in front of many LLM providers"
)]
pub(super) struct Cli {
    /// Interface to bind [default: 127.0.0.1]
    #[arg(long, env = HOST, value_name = "ADDR")]
    host: Option<String>,
    /// Port to listen on [default: 8787]
    #[arg(long, env = PORT, value_name = "PORT")]
    port: Option<String>,
    /// Directory for the database and other state [default: ./data]
    #[arg(long, env = DATA_DIR, value_name = "PATH")]
    data_dir: Option<String>,
    /// Persistence backend: sqlite, libsql, postgres, or mysql [default: sqlite]
    #[arg(long, env = PERSISTENCE, value_name = "BACKEND")]
    persistence: Option<String>,
    /// PostgreSQL or MySQL connection string
    #[arg(long, env = DSN, value_name = "DSN", hide_env_values = true)]
    dsn: Option<String>,
    /// libSQL endpoint; required only for the `libsql` backend
    #[arg(long, env = LIBSQL_URL, value_name = "URL")]
    libsql_url: Option<String>,
    /// libSQL auth token; required only for the `libsql` backend
    #[arg(long, env = LIBSQL_AUTH_TOKEN, value_name = "TOKEN", hide_env_values = true)]
    libsql_auth_token: Option<String>,
    /// Redis URL for the shared cache; omit for the persistence-backed/default cache
    #[arg(long, env = REDIS_URL, value_name = "URL", hide_env_values = true)]
    redis_url: Option<String>,
    /// Upstash Redis REST URL
    #[arg(long, env = UPSTASH_URL, value_name = "URL", hide_env_values = true)]
    upstash_url: Option<String>,
    /// Upstash Redis REST token
    #[arg(long, env = UPSTASH_TOKEN, value_name = "TOKEN", hide_env_values = true)]
    upstash_token: Option<String>,
    /// Base64 key the store is sealed with. Unset means secrets are stored
    /// in plaintext, which is the right choice when the database itself is
    /// trusted
    #[arg(long, env = MASTER_KEY, value_name = "BASE64", hide_env_values = true)]
    master_key: Option<String>,
    /// Key to rotate to; an empty value rotates the store back to plaintext.
    /// Takes effect only together with --master-key-rotate
    #[arg(long, env = MASTER_KEY_NEXT, value_name = "BASE64", hide_env_values = true)]
    master_key_next: Option<String>,
    /// Arm the rotation. Without it a stray empty --master-key-next cannot
    /// silently decrypt the whole store
    #[arg(long, env = MASTER_KEY_ROTATE, value_name = "BOOL")]
    master_key_rotate: Option<String>,
    /// Default outbound proxy. Credential and provider settings override it.
    /// Ambient HTTP_PROXY/HTTPS_PROXY are ignored unless the persisted
    /// `inherit_system_proxy` instance setting is enabled
    #[arg(long, env = UPSTREAM_PROXY_URL, value_name = "URL", hide_env_values = true)]
    upstream_proxy_url: Option<String>,
    /// Numeric id included in native request identifiers [default: 0]
    #[arg(long, env = INSTANCE_ID, value_name = "ID")]
    instance_id: Option<String>,
    /// Maximum upstream candidates attempted per request [default: 6]
    #[arg(long, env = MAX_ATTEMPTS, value_name = "COUNT")]
    max_attempts: Option<String>,
    /// Maximum concurrent gateway requests [default: 1024]
    #[arg(long, env = MAX_IN_FLIGHT, value_name = "COUNT")]
    max_in_flight: Option<String>,
    /// Process upload concurrency override; 0 means unlimited
    #[arg(long, env = FILE_UPLOAD_MAX_IN_FLIGHT, value_name = "COUNT")]
    file_upload_max_in_flight: Option<String>,
    /// Trusted reverse-proxy IPs, comma-separated; loopback is always trusted
    #[arg(long = "trusted-proxy", env = TRUSTED_PROXIES, value_name = "IP")]
    trusted_proxies: Option<String>,
    /// Exact allowed browser origins, comma-separated; empty is same-origin
    #[arg(long = "cors-origin", env = CORS_ORIGINS, value_name = "ORIGIN")]
    cors_origins: Option<String>,
    /// Native log format: text or newline-delimited json [default: text]
    #[arg(long, env = LOG_FORMAT, value_name = "FORMAT")]
    log_format: Option<String>,
    /// First-run administrator username [default: admin]
    #[arg(long, env = ADMIN_USER, value_name = "USER")]
    admin_user: Option<String>,
    /// First-run administrator password. Existing accounts are never changed
    #[arg(long, env = ADMIN_PASSWORD, value_name = "PASSWORD", hide_env_values = true)]
    admin_password: Option<String>,
    /// First-run administrator API key. Existing stores are never changed
    #[arg(long, env = BOOTSTRAP_ADMIN_API_KEY, value_name = "KEY", hide_env_values = true)]
    bootstrap_admin_api_key: Option<String>,
    /// Channel ids to create on first run, comma-separated
    #[arg(long = "bootstrap-channel", env = BOOTSTRAP_CHANNELS, value_name = "CHANNEL")]
    bootstrap_channels: Option<String>,
}

pub(super) fn load() -> Result<Config, ConfigError> {
    let cwd = std::env::current_dir().map_err(environment_error)?;
    resolve(<Cli as clap::Parser>::parse(), &cwd)
}

fn resolve(cli: Cli, cwd: &Path) -> Result<Config, ConfigError> {
    let mut dotenv = read_dotenv(&cwd.join(".env"))?;
    let data_dir = cli
        .data_dir
        .or_else(|| dotenv.remove(DATA_DIR))
        .map_or_else(|| PathBuf::from("data"), PathBuf::from);
    let data_dir = if data_dir.is_absolute() {
        data_dir
    } else {
        cwd.join(data_dir)
    };
    let data_env = data_dir.join(".env");
    if data_env != cwd.join(".env") {
        for (key, value) in read_dotenv(&data_env)? {
            dotenv.entry(key).or_insert(value);
        }
    }
    let layered = |cli: Option<String>, name: &str| cli.or_else(|| dotenv.get(name).cloned());

    let host = layered(cli.host, HOST).unwrap_or_else(|| "127.0.0.1".into());
    let port = layered(cli.port, PORT).unwrap_or_else(|| "8787".into());
    let listen_addr = format!("{host}:{port}")
        .parse()
        .map_err(|error| invalid(HOST, format!("expected IP address and port: {error}")))?;
    let secret_keys = MasterKeyConfig::from_encoded(
        layered(cli.master_key, MASTER_KEY),
        layered(cli.master_key_next, MASTER_KEY_NEXT),
        rotation_enabled(layered(cli.master_key_rotate, MASTER_KEY_ROTATE))?,
    )?;
    let native = NativeOptions {
        upstream_proxy_url: nonempty(layered(cli.upstream_proxy_url, UPSTREAM_PROXY_URL)),
        instance_id: parse_number(layered(cli.instance_id, INSTANCE_ID), INSTANCE_ID, 0, true)?,
        max_attempts: parse_number(
            layered(cli.max_attempts, MAX_ATTEMPTS),
            MAX_ATTEMPTS,
            6,
            false,
        )?,
        max_in_flight: parse_number(
            layered(cli.max_in_flight, MAX_IN_FLIGHT),
            MAX_IN_FLIGHT,
            1024,
            false,
        )?,
        file_upload_max_in_flight: layered(
            cli.file_upload_max_in_flight,
            FILE_UPLOAD_MAX_IN_FLIGHT,
        )
        .map(|value| parse_value(&value, FILE_UPLOAD_MAX_IN_FLIGHT, true))
        .transpose()?,
        trusted_proxies: parse_list(
            layered(cli.trusted_proxies, TRUSTED_PROXIES),
            TRUSTED_PROXIES,
        )?,
        cors_origins: split_list(layered(cli.cors_origins, CORS_ORIGINS)),
        log_format: match layered(cli.log_format, LOG_FORMAT)
            .unwrap_or_else(|| "text".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "text" => LogFormat::Text,
            "json" => LogFormat::Json,
            _ => return Err(invalid(LOG_FORMAT, "expected `text` or `json`")),
        },
        admin_user: layered(cli.admin_user, ADMIN_USER).unwrap_or_else(|| "admin".into()),
        admin_password: layered(cli.admin_password, ADMIN_PASSWORD),
        bootstrap_admin_api_key: layered(cli.bootstrap_admin_api_key, BOOTSTRAP_ADMIN_API_KEY),
        bootstrap_channels: split_list(layered(cli.bootstrap_channels, BOOTSTRAP_CHANNELS)),
    };
    let persistence = layered(cli.persistence, PERSISTENCE)
        .unwrap_or_else(|| "sqlite".into())
        .to_ascii_lowercase();
    let config = match persistence.as_str() {
        "sqlite" => Ok(Config::sqlite(listen_addr, data_dir, secret_keys)),
        "libsql" => Config::libsql(
            listen_addr,
            data_dir,
            layered(cli.libsql_url, LIBSQL_URL).unwrap_or_default(),
            layered(cli.libsql_auth_token, LIBSQL_AUTH_TOKEN).unwrap_or_default(),
            secret_keys,
        ),
        "postgres" | "mysql" => Config::sqlite(listen_addr, data_dir, secret_keys).sql_server(
            if persistence == "postgres" {
                "postgres"
            } else {
                "mysql"
            },
            layered(cli.dsn, DSN).unwrap_or_default(),
        ),
        _ => Err(invalid(
            PERSISTENCE,
            "expected `sqlite`, `libsql`, `postgres`, or `mysql`",
        )),
    }?;
    let redis_url = nonempty(layered(cli.redis_url, REDIS_URL));
    let upstash_url = nonempty(layered(cli.upstash_url, UPSTASH_URL));
    let upstash_token = nonempty(layered(cli.upstash_token, UPSTASH_TOKEN));
    let cache = if let Some(url) = redis_url {
        CacheConfig::Redis { url }
    } else {
        match (upstash_url, upstash_token) {
            (Some(url), Some(token)) => {
                return Ok(config.with_upstash(url, token)?.with_native_options(native));
            }
            (None, None) if persistence == "libsql" => CacheConfig::Libsql,
            (None, None) => CacheConfig::InProcess,
            _ => {
                return Err(invalid(
                    UPSTASH_URL,
                    "UPSTASH_URL and UPSTASH_TOKEN must be set together",
                ));
            }
        }
    };
    Ok(config.with_cache(cache).with_native_options(native))
}

fn nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_number<T>(
    value: Option<String>,
    name: &'static str,
    default: T,
    zero_allowed: bool,
) -> Result<T, ConfigError>
where
    T: std::str::FromStr + Default + PartialEq,
{
    value.map_or(Ok(default), |value| parse_value(&value, name, zero_allowed))
}

fn parse_value<T>(value: &str, name: &'static str, zero_allowed: bool) -> Result<T, ConfigError>
where
    T: std::str::FromStr + Default + PartialEq,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| invalid(name, "expected a non-negative integer"))?;
    if !zero_allowed && parsed == T::default() {
        return Err(invalid(name, "must be positive"));
    }
    Ok(parsed)
}

fn split_list(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_list<T>(value: Option<String>, name: &'static str) -> Result<Vec<T>, ConfigError>
where
    T: std::str::FromStr,
{
    split_list(value)
        .into_iter()
        .map(|value| {
            value
                .parse()
                .map_err(|_| invalid(name, "contains an invalid value"))
        })
        .collect()
}

/// Only GPROXY bootstrap keys and v2's two `UPSTASH_*` names are taken from a
/// `.env`. Unrelated deployment tokens routinely share that file and remain
/// outside this process.
fn read_dotenv(path: &Path) -> Result<HashMap<String, String>, ConfigError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(environment_error(error)),
    };
    let mut values = HashMap::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.split_once('#').map_or(line, |(value, _)| value).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| {
            ConfigError::Environment(format!(
                "{}:{}: expected KEY=value",
                path.display(),
                index + 1
            ))
        })?;
        let key = key.trim();
        if key.is_empty() {
            return Err(ConfigError::Environment(format!(
                "{}:{}: environment key is empty",
                path.display(),
                index + 1
            )));
        }
        if !key.starts_with("GPROXY_") && !matches!(key, UPSTASH_URL | UPSTASH_TOKEN) {
            continue;
        }
        values.insert(key.to_owned(), value.trim().to_owned());
    }
    Ok(values)
}

fn rotation_enabled(value: Option<String>) -> Result<bool, ConfigError> {
    match value.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
        None => Ok(false),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on") => Ok(true),
        Some(value) if matches!(value.as_str(), "" | "0" | "false" | "no" | "off") => Ok(false),
        Some(_) => Err(invalid(
            MASTER_KEY_ROTATE,
            "expected one of `1`, `true`, `yes`, `on`, `0`, `false`, `no`, or `off`",
        )),
    }
}

fn environment_error(error: impl std::fmt::Display) -> ConfigError {
    ConfigError::Environment(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use super::*;

    /// Mirrors what clap's `env` does, so the test exercises the same
    /// layering without depending on the real process environment.
    fn from_environment(environment: &HashMap<&str, OsString>) -> Cli {
        let get = |name: &str| {
            environment
                .get(name)
                .and_then(|value| value.clone().into_string().ok())
        };
        Cli {
            host: get(HOST),
            port: get(PORT),
            data_dir: get(DATA_DIR),
            persistence: get(PERSISTENCE),
            dsn: get(DSN),
            libsql_url: get(LIBSQL_URL),
            libsql_auth_token: get(LIBSQL_AUTH_TOKEN),
            redis_url: get(REDIS_URL),
            upstash_url: get(UPSTASH_URL),
            upstash_token: get(UPSTASH_TOKEN),
            master_key: get(MASTER_KEY),
            master_key_next: get(MASTER_KEY_NEXT),
            master_key_rotate: get(MASTER_KEY_ROTATE),
            upstream_proxy_url: get(UPSTREAM_PROXY_URL),
            instance_id: get(INSTANCE_ID),
            max_attempts: get(MAX_ATTEMPTS),
            max_in_flight: get(MAX_IN_FLIGHT),
            file_upload_max_in_flight: get(FILE_UPLOAD_MAX_IN_FLIGHT),
            trusted_proxies: get(TRUSTED_PROXIES),
            cors_origins: get(CORS_ORIGINS),
            log_format: get(LOG_FORMAT),
            admin_user: get(ADMIN_USER),
            admin_password: get(ADMIN_PASSWORD),
            bootstrap_admin_api_key: get(BOOTSTRAP_ADMIN_API_KEY),
            bootstrap_channels: get(BOOTSTRAP_CHANNELS),
        }
    }

    #[test]
    fn dotenv_parsing_uses_real_environment_first() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join(".env"),
            "# comment\nGPROXY_PORT=1000 # inline\nGPROXY_DATA_DIR=data-home\nDENO_DEPLOY_TOKEN=not-ours\n",
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("data-home")).unwrap();
        std::fs::write(
            directory.path().join("data-home/.env"),
            "GPROXY_PORT=2000\nGPROXY_PERSISTENCE=sqlite\n",
        )
        .unwrap();
        let environment = HashMap::from([(PORT, OsString::from("3000"))]);
        let config = resolve(from_environment(&environment), directory.path()).unwrap();
        assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
        assert_eq!(config.data_dir(), directory.path().join("data-home"));
        assert!(
            !read_dotenv(&directory.path().join(".env"))
                .unwrap()
                .contains_key("DENO_DEPLOY_TOKEN")
        );
    }

    #[test]
    fn command_line_wins_over_environment_and_dotenv() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(".env"), "GPROXY_PORT=1000\n").unwrap();
        let mut cli = from_environment(&HashMap::from([(PORT, OsString::from("2000"))]));
        cli.port = Some("3000".into());
        let config = resolve(cli, directory.path()).unwrap();
        assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
    }
}
