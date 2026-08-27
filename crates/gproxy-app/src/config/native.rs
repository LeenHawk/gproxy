use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Config, SecretKeyConfig, invalid};
use crate::ConfigError;

const LISTEN_ADDR: &str = "GPROXY_LISTEN_ADDR";
const DATA_DIR: &str = "GPROXY_DATA_DIR";
const STORE_BACKEND: &str = "GPROXY_STORE_BACKEND";
const LIBSQL_URL: &str = "GPROXY_LIBSQL_URL";
const LIBSQL_AUTH_TOKEN: &str = "GPROXY_LIBSQL_AUTH_TOKEN";
const SECRET_KEY: &str = "GPROXY_SECRET_KEY";
const SECRET_KEY_NEXT: &str = "GPROXY_SECRET_KEY_NEXT";
const SECRET_KEY_ROTATE: &str = "GPROXY_SECRET_KEY_ROTATE";

/// The whole configuration surface, declared once. `--help` is generated
/// from this, so the flag list and the environment list cannot drift apart.
/// Every value is optional here because `.env` and the defaults are layered
/// underneath: command line, then real environment, then `.env`, then the
/// default.
#[derive(Debug, Default, clap::Parser)]
#[command(
    name = "gproxy",
    version,
    about = "GPROXY — one gateway in front of many LLM providers"
)]
pub(super) struct Cli {
    /// Address to listen on [default: 127.0.0.1:8787]
    #[arg(long, env = LISTEN_ADDR, value_name = "ADDR")]
    listen_addr: Option<String>,
    /// Directory for the database and other state [default: ./data]
    #[arg(long, env = DATA_DIR, value_name = "PATH")]
    data_dir: Option<String>,
    /// Persistence backend: `sqlite` or `libsql` [default: sqlite]
    #[arg(long, env = STORE_BACKEND, value_name = "BACKEND")]
    store_backend: Option<String>,
    /// libSQL endpoint; required only for the `libsql` backend
    #[arg(long, env = LIBSQL_URL, value_name = "URL")]
    libsql_url: Option<String>,
    /// libSQL auth token; required only for the `libsql` backend
    #[arg(long, env = LIBSQL_AUTH_TOKEN, value_name = "TOKEN", hide_env_values = true)]
    libsql_auth_token: Option<String>,
    /// Base64 key the store is sealed with. Unset means secrets are stored
    /// in plaintext, which is the right choice when the database itself is
    /// trusted
    #[arg(long, env = SECRET_KEY, value_name = "BASE64", hide_env_values = true)]
    secret_key: Option<String>,
    /// Key to rotate to; an empty value rotates the store back to plaintext.
    /// Takes effect only together with --secret-key-rotate
    #[arg(long, env = SECRET_KEY_NEXT, value_name = "BASE64", hide_env_values = true)]
    secret_key_next: Option<String>,
    /// Arm the rotation. Without it a stray empty --secret-key-next cannot
    /// silently decrypt the whole store
    #[arg(long, env = SECRET_KEY_ROTATE, value_name = "BOOL")]
    secret_key_rotate: Option<String>,
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

    let listen_addr = layered(cli.listen_addr, LISTEN_ADDR)
        .unwrap_or_else(|| "127.0.0.1:8787".into())
        .parse()
        .map_err(|error| invalid(LISTEN_ADDR, format!("expected IP socket address: {error}")))?;
    let secret_keys = SecretKeyConfig::from_encoded(
        layered(cli.secret_key, SECRET_KEY),
        layered(cli.secret_key_next, SECRET_KEY_NEXT),
        rotation_enabled(layered(cli.secret_key_rotate, SECRET_KEY_ROTATE))?,
    )?;
    match layered(cli.store_backend, STORE_BACKEND)
        .unwrap_or_else(|| "sqlite".into())
        .as_str()
    {
        "sqlite" => Ok(Config::sqlite(listen_addr, data_dir, secret_keys)),
        "libsql" => Config::libsql(
            listen_addr,
            data_dir,
            layered(cli.libsql_url, LIBSQL_URL).unwrap_or_default(),
            layered(cli.libsql_auth_token, LIBSQL_AUTH_TOKEN).unwrap_or_default(),
            secret_keys,
        ),
        _ => Err(invalid(STORE_BACKEND, "expected `sqlite` or `libsql`")),
    }
}

/// Only `GPROXY_*` keys are taken from a `.env`. Deployment tokens and other
/// unrelated secrets routinely share that file; reading them into this
/// process would be a promise we have no reason to make.
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
        if !key.starts_with("GPROXY_") {
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
            SECRET_KEY_ROTATE,
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
            listen_addr: get(LISTEN_ADDR),
            data_dir: get(DATA_DIR),
            store_backend: get(STORE_BACKEND),
            libsql_url: get(LIBSQL_URL),
            libsql_auth_token: get(LIBSQL_AUTH_TOKEN),
            secret_key: get(SECRET_KEY),
            secret_key_next: get(SECRET_KEY_NEXT),
            secret_key_rotate: get(SECRET_KEY_ROTATE),
        }
    }

    #[test]
    fn dotenv_parsing_uses_real_environment_first() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join(".env"),
            "# comment\nGPROXY_LISTEN_ADDR=127.0.0.1:1000 # inline\nGPROXY_DATA_DIR=data-home\nDENO_DEPLOY_TOKEN=not-ours\n",
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("data-home")).unwrap();
        std::fs::write(
            directory.path().join("data-home/.env"),
            "GPROXY_LISTEN_ADDR=127.0.0.1:2000\nGPROXY_STORE_BACKEND=sqlite\n",
        )
        .unwrap();
        let environment = HashMap::from([(LISTEN_ADDR, OsString::from("127.0.0.1:3000"))]);
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
        std::fs::write(
            directory.path().join(".env"),
            "GPROXY_LISTEN_ADDR=127.0.0.1:1000\n",
        )
        .unwrap();
        let mut cli = from_environment(&HashMap::from([(
            LISTEN_ADDR,
            OsString::from("127.0.0.1:2000"),
        )]));
        cli.listen_addr = Some("127.0.0.1:3000".into());
        let config = resolve(cli, directory.path()).unwrap();
        assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
    }
}
