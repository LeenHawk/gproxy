//! Configuration layering, end to end: the flag, the environment, `.env`, the
//! file, the default.
//!
//! # Why the environment is touched here and only here
//!
//! `clap` reads the real process environment when it parses, so a test of "the
//! environment beats the file" has to set a real variable — there is no seam to
//! inject. `std::env::set_var` is `unsafe` because it races any other thread
//! reading the environment, so every test in this file takes [`ENVIRONMENT`]
//! first and puts the variables back afterwards. Integration test files are
//! separate processes, so nothing outside this one is affected.
//!
//! The pure layering rules — parsing, backend selection, the master key — are
//! unit-tested in `src/config.rs`, where no environment is involved at all. What
//! is left for this file is the part that only exists once a process is
//! involved: that the four sources really do rank the way the module claims.

use std::sync::{Mutex, MutexGuard, OnceLock};

use clap::Parser;
use gproxy::{Cli, config};

/// Serializes every test that touches the process environment.
fn environment() -> MutexGuard<'static, ()> {
    static ENVIRONMENT: OnceLock<Mutex<()>> = OnceLock::new();
    ENVIRONMENT
        .get_or_init(|| Mutex::new(()))
        // A panicking test must not make every later one fail on a poisoned
        // lock: the environment is restored by `Restore` on unwind either way.
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Remembers what the named variables were and puts them back on drop, even if
/// the test panics.
struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Restore {
    /// Remember, without changing anything. For a test that needs a variable
    /// *absent* so that `.env` can supply it.
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }

    fn set(pairs: &[(&'static str, &str)]) -> Self {
        let restore = Self::capture(&pairs.iter().map(|(key, _)| *key).collect::<Vec<_>>());
        for (key, value) in pairs {
            set(key, value);
        }
        restore
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            match value {
                Some(value) => set(key, &value.to_string_lossy()),
                None => clear(key),
            }
        }
    }
}

/// Safe here, and only here: [`environment`] is held for the whole test and this
/// process spawns no thread that reads the environment.
fn set(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

fn clear(key: &str) {
    unsafe { std::env::remove_var(key) };
}

fn config_file(body: &str) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut file = tempfile::Builder::new()
        .suffix(".toml")
        .tempfile()
        .expect("temporary file");
    file.write_all(body.as_bytes()).expect("write");
    file.flush().expect("flush");
    file
}

// ------------------------------------------------------- the four rungs --

#[test]
fn a_flag_beats_the_environment() {
    let _guard = environment();
    let _restore = Restore::set(&[("GPROXY_PORT", "2000")]);

    let cli = Cli::try_parse_from(["gproxy", "--port", "3000", "serve"]).unwrap();
    assert_eq!(config::settings(&cli).unwrap().config.port, 3000);
}

#[test]
fn the_environment_beats_the_file() {
    let _guard = environment();
    let _restore = Restore::set(&[("GPROXY_PORT", "2000")]);
    let file = config_file("port = 1000\n");

    let cli = Cli::try_parse_from([
        "gproxy",
        "--config",
        &file.path().to_string_lossy(),
        "serve",
    ])
    .unwrap();
    assert_eq!(config::settings(&cli).unwrap().config.port, 2000);
}

#[test]
fn a_dotenv_beats_the_file_and_loses_to_the_environment() {
    let _guard = environment();
    let _restore = Restore::capture(&["GPROXY_HOST", "GPROXY_PORT"]);
    // HOST is absent, so `.env` gets to supply it. PORT is set for real, so
    // `.env` must not be allowed to touch it.
    clear("GPROXY_HOST");
    set("GPROXY_PORT", "2000");

    let dotenv = config_file("GPROXY_HOST=10.1.2.3\nGPROXY_PORT=1500\n");
    gproxy::env::load(dotenv.path()).unwrap();

    let file = config_file("host = \"10.9.9.9\"\nport = 1000\n");
    let cli = Cli::try_parse_from([
        "gproxy",
        "--config",
        &file.path().to_string_lossy(),
        "serve",
    ])
    .unwrap();
    let settings = config::settings(&cli).unwrap();
    // `.env` filled what nothing else had, beating the file.
    assert_eq!(settings.config.host, "10.1.2.3");
    // And it did not overwrite what the real environment already said.
    assert_eq!(settings.config.port, 2000);
}

#[test]
fn the_file_beats_the_default() {
    let _guard = environment();
    let _restore = Restore::capture(&["GPROXY_PORT", "GPROXY_HOST"]);
    clear("GPROXY_PORT");
    clear("GPROXY_HOST");
    let file = config_file("port = 1000\nhost = \"0.0.0.0\"\n");

    let cli = Cli::try_parse_from([
        "gproxy",
        "--config",
        &file.path().to_string_lossy(),
        "serve",
    ])
    .unwrap();
    let settings = config::settings(&cli).unwrap();
    assert_eq!(settings.config.port, 1000);
    assert_eq!(settings.config.host, "0.0.0.0");
}

#[test]
fn the_default_is_what_is_left() {
    let _guard = environment();
    let keys = [
        "GPROXY_PORT",
        "GPROXY_HOST",
        "GPROXY_DATA_DIR",
        "GPROXY_CONFIG",
    ];
    let _restore = Restore::capture(&keys);
    for key in keys {
        clear(key);
    }

    let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
    let settings = config::settings(&cli).unwrap();
    assert_eq!(settings.config.host, "127.0.0.1");
    assert_eq!(settings.config.port, 7070);
    assert_eq!(
        settings.config.data_dir.as_deref(),
        Some(config::DEFAULT_DATA_DIR)
    );
}

// -------------------------------------------------------- the master key --

#[test]
fn a_master_key_is_read_from_the_environment_in_either_encoding() {
    use base64::Engine;
    let _guard = environment();
    let raw = [0x3c_u8; 32];
    let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
    let base64 = base64::engine::general_purpose::STANDARD.encode(raw);

    for encoding in [hex, base64] {
        let _restore = Restore::set(&[("GPROXY_MASTER_KEY", encoding.as_str())]);
        let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
        let settings = config::settings(&cli).unwrap();
        assert_eq!(
            settings.config.master_key.resolve().unwrap(),
            Some(raw),
            "{encoding}"
        );
    }
}

#[test]
fn a_master_key_of_the_wrong_length_is_refused_before_anything_opens() {
    use base64::Engine;
    let _guard = environment();
    // Valid base64 of the wrong number of bytes: the encoding is fine and the
    // key is not, which is the mistake worth a precise message.
    let short = base64::engine::general_purpose::STANDARD.encode([0_u8; 16]);
    let _restore = Restore::set(&[("GPROXY_MASTER_KEY", short.as_str())]);

    let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
    let error = config::settings(&cli).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("--master-key / GPROXY_MASTER_KEY"),
        "{error}"
    );
    assert!(error.to_string().contains("got 16"), "{error}");
}

#[test]
fn a_master_key_that_is_not_an_encoding_at_all_is_refused() {
    let _guard = environment();
    let _restore = Restore::set(&[("GPROXY_MASTER_KEY", "not a key!!")]);

    let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
    let error = config::settings(&cli).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("--master-key / GPROXY_MASTER_KEY"),
        "{error}"
    );
}

#[test]
fn rotation_is_only_armed_by_its_own_variable() {
    let _guard = environment();
    let key = "11".repeat(32);
    let next = "22".repeat(32);

    let _restore = Restore::set(&[
        ("GPROXY_MASTER_KEY", key.as_str()),
        ("GPROXY_MASTER_KEY_NEXT", next.as_str()),
        ("GPROXY_MASTER_KEY_ROTATE", ""),
    ]);
    clear("GPROXY_MASTER_KEY_ROTATE");
    let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
    let settings = config::settings(&cli).unwrap();
    assert!(!settings.config.master_key.rotate);
    assert_eq!(settings.config.master_key.resolve_next().unwrap(), None);

    set("GPROXY_MASTER_KEY_ROTATE", "true");
    let cli = Cli::try_parse_from(["gproxy", "serve"]).unwrap();
    let settings = config::settings(&cli).unwrap();
    assert!(settings.config.master_key.rotate);
    assert_eq!(
        settings.config.master_key.resolve_next().unwrap(),
        Some([0x22_u8; 32])
    );
}

// --------------------------------------------------------------- the file --

#[test]
fn a_config_file_carries_what_no_flag_exposes() {
    let _guard = environment();
    // The file speaks `AppConfig`'s own field names, which are snake_case: the
    // type is `#[serde(deny_unknown_fields)]` with no renaming, so a TOML key is
    // the Rust field.
    let file = config_file(
        r#"
session_ttl_secs = 3600

[oauth]
access_ttl_secs = 60
cli_client_ids = ["codex"]

[file_storage]
kind = "s3"
bucket = "gproxy"
region = "auto"
"#,
    );
    let cli = Cli::try_parse_from(["gproxy", "--config", &file.path().to_string_lossy()]).unwrap();
    let settings = config::settings(&cli).unwrap();
    assert_eq!(settings.config.session_ttl_secs, 3600);
    assert_eq!(settings.config.oauth.access_ttl_secs, 60);
    assert_eq!(
        settings.config.oauth.cli_client_ids,
        vec!["codex".to_owned()]
    );
    assert!(settings.config.file_storage.is_some());
    // And what the file did not mention keeps its default.
    assert_eq!(settings.config.oauth.code_ttl_secs, 300);
}

#[test]
fn a_typo_in_the_file_is_reported_rather_than_ignored() {
    let _guard = environment();
    let file = config_file("prot = 9000\n");

    let cli = Cli::try_parse_from(["gproxy", "--config", &file.path().to_string_lossy()]).unwrap();
    let error = config::settings(&cli).unwrap_err();
    assert!(
        error.to_string().starts_with("--config / GPROXY_CONFIG"),
        "{error}"
    );
}

#[test]
fn a_config_file_that_is_not_there_is_an_error_because_it_was_asked_for() {
    let _guard = environment();
    let cli = Cli::try_parse_from(["gproxy", "--config", "/nonexistent/gproxy.toml"]).unwrap();
    let error = config::settings(&cli).unwrap_err();
    assert!(error.to_string().contains("gproxy.toml"), "{error}");
}
