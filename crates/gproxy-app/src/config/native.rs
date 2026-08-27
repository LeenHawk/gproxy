use std::collections::HashMap;
use std::ffi::OsString;
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

pub(super) fn load() -> Result<Config, ConfigError> {
    let cwd = std::env::current_dir().map_err(environment_error)?;
    load_from(&cwd, |name| std::env::var_os(name))
}

fn load_from(
    cwd: &Path,
    environment: impl Fn(&str) -> Option<OsString>,
) -> Result<Config, ConfigError> {
    let mut dotenv = read_dotenv(&cwd.join(".env"))?;
    let data_dir = value(DATA_DIR, &dotenv, &environment)?
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));
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

    let listen_addr = value(LISTEN_ADDR, &dotenv, &environment)?
        .unwrap_or_else(|| "127.0.0.1:8787".into())
        .parse()
        .map_err(|error| invalid(LISTEN_ADDR, format!("expected IP socket address: {error}")))?;
    let secret_keys = SecretKeyConfig::from_encoded(
        value(SECRET_KEY, &dotenv, &environment)?,
        value(SECRET_KEY_NEXT, &dotenv, &environment)?,
        rotation_enabled(value(SECRET_KEY_ROTATE, &dotenv, &environment)?)?,
    )?;
    match value(STORE_BACKEND, &dotenv, &environment)?
        .unwrap_or_else(|| "sqlite".into())
        .as_str()
    {
        "sqlite" => Ok(Config::sqlite(listen_addr, data_dir, secret_keys)),
        "libsql" => Config::libsql(
            listen_addr,
            data_dir,
            value(LIBSQL_URL, &dotenv, &environment)?.unwrap_or_default(),
            value(LIBSQL_AUTH_TOKEN, &dotenv, &environment)?.unwrap_or_default(),
            secret_keys,
        ),
        _ => Err(invalid(STORE_BACKEND, "expected `sqlite` or `libsql`")),
    }
}

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
        values.insert(key.to_owned(), value.trim().to_owned());
    }
    Ok(values)
}

fn value(
    name: &'static str,
    dotenv: &HashMap<String, String>,
    environment: &impl Fn(&str) -> Option<OsString>,
) -> Result<Option<String>, ConfigError> {
    match environment(name) {
        Some(value) => value
            .into_string()
            .map(Some)
            .map_err(|_| invalid(name, "environment value is not UTF-8")),
        None => Ok(dotenv.get(name).cloned()),
    }
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

    use super::*;

    #[test]
    fn dotenv_parsing_uses_real_environment_first() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join(".env"),
            "# comment\nGPROXY_LISTEN_ADDR=127.0.0.1:1000 # inline\nGPROXY_DATA_DIR=data-home\n",
        )
        .unwrap();
        std::fs::create_dir(directory.path().join("data-home")).unwrap();
        std::fs::write(
            directory.path().join("data-home/.env"),
            "GPROXY_LISTEN_ADDR=127.0.0.1:2000\nGPROXY_STORE_BACKEND=sqlite\n",
        )
        .unwrap();
        let environment = HashMap::from([(LISTEN_ADDR, OsString::from("127.0.0.1:3000"))]);
        let config = load_from(directory.path(), |name| environment.get(name).cloned()).unwrap();
        assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
        assert_eq!(config.data_dir(), directory.path().join("data-home"));
    }
}
