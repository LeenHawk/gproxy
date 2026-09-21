//! What a desktop instance is configured with, and the two things it does not
//! inherit from the command line.
//!
//! [`gproxy::config`] layers a command line over an environment over a `.env`
//! over a TOML file. A window has no command line and no shell environment
//! worth reading, so that layering does not apply here — but the *type* it
//! produces does. This module builds the same [`gproxy::Settings`] from two
//! sources instead of five:
//!
//! 1. `gproxy.toml` in the data directory, in the schema
//!    [`AppConfig`](gproxy_app::AppConfig) already has;
//! 2. the desktop's own defaults.
//!
//! Reusing `AppConfig`'s serde rather than inventing a desktop schema is the
//! point. Every field the server understands is settable here, spelled the way
//! the server spells it, and there is no second document to keep in step.
//!
//! # The three fields the shell decides for you
//!
//! `data_dir`, the store and the console are not read from the file, because a
//! desktop instance is not free to disagree about them:
//!
//! - **`data_dir`** is the platform's application-data directory, handed in by
//!   Tauri. A file inside it cannot choose where it is.
//! - **the store** is a SQLite file in that directory. A desktop app pointed at
//!   somebody's production Postgres is not a thing this shell offers.
//! - **the console** is the window. The embedded HTTP host serves no console
//!   bundle, so the switch is off and stays off.
//!
//! # The port
//!
//! The data plane's clients are Claude Code and the Codex CLI, and they are
//! configured with a base URL that is typed once and kept. So the default is a
//! **fixed** port, [`DEFAULT_PORT`], rather than a random high one: a port that
//! moved on every launch would mean editing every client's configuration on
//! every launch, which is the opposite of what a desktop shell is for.
//!
//! It is 7071 rather than the server's 7070 so that a developer can run
//! `gproxy serve` and the desktop shell side by side without either one
//! failing to bind. Set `port` in `gproxy.toml` to change it.

use std::path::{Path, PathBuf};

use gproxy::config::{AdminOptions, Settings, TelemetryOptions};
use gproxy_app::{
    AppConfig,
    config::{ConsoleConfig, FileStorageConfig, StoreBackendConfig},
};

use crate::{StartError, StartResult, secrets::MasterKeyOutcome};

/// The loopback port the embedded data plane binds by default.
pub const DEFAULT_PORT: u16 = 7071;

/// The only address a desktop data plane binds. Not configurable: an instance
/// whose keys live in this user's keychain has no business listening on a
/// network interface, and an operator who wants that wants `gproxy serve`.
pub const LOOPBACK: &str = "127.0.0.1";

/// The SQLite file inside the data directory.
pub const DATABASE_FILE: &str = "gproxy.db";

/// The optional configuration file, read from the data directory.
pub const CONFIG_FILE: &str = "gproxy.toml";

/// The local storage root for published bodies and downloaded vocabularies.
///
/// Enabled by default, unlike the server's, because there is no operator here
/// to turn it on and the features that need it — a vocabulary download, an
/// upstream handed a fetchable link — simply fail without it.
const FILE_STORAGE_DIR: &str = "files";

/// The administrator this shell runs as.
pub const DESKTOP_ADMIN_USER: &str = "desktop";

/// Build the settings for the instance rooted at `data_dir`.
///
/// `master_key` comes from [`crate::secrets`] and is dropped in whole: the
/// rotation and the codec choice stay in `gproxy::instance`, which is the one
/// place that knows how to do them.
pub fn settings(data_dir: &Path, master_key: MasterKeyOutcome) -> StartResult<Settings> {
    let mut config = read_file(data_dir)?;

    config.data_dir = Some(data_dir.to_string_lossy().into_owned());
    config.store = StoreBackendConfig::Sqlite {
        path: DATABASE_FILE.to_owned(),
    };
    config.host = LOOPBACK.to_owned();
    config.console = ConsoleConfig {
        enabled: false,
        path: None,
    };
    if config.file_storage.is_none() {
        config.file_storage = Some(FileStorageConfig::Fs {
            root: FILE_STORAGE_DIR.to_owned(),
        });
    }
    config.master_key = master_key.config;

    Ok(Settings {
        config,
        admin: AdminOptions {
            user: DESKTOP_ADMIN_USER.to_owned(),
            // Neither of the operator's two options is taken, and the two
            // `None`s mean different things.
            //
            // `password: None` asks bootstrap to generate one. This shell then
            // never shows it — `Report::announce` is the command line's
            // presentation, and `Desktop::start` does not call it — so the
            // account has a password that nobody, including this process,
            // knows. That is on purpose. Nobody signs in here: the window
            // reaches the management surfaces over IPC. The browser door on
            // the loopback socket is shut anyway (the embedded host refuses
            // `/portal/api`), and an unguessable secret behind a shut door is
            // a better resting state than no secret at all. A person who
            // wants that door opened calls `admin_users_set_password` and
            // chooses one.
            //
            // `api_key: None` asks bootstrap to mint one, which the shell
            // takes from the report and puts in the keychain; see
            // [`crate::desktop`].
            password: None,
            api_key: None,
        },
        telemetry: TelemetryOptions::default(),
        instance_id: None,
    })
}

/// The data directory's `gproxy.toml`, or the defaults.
///
/// A file that is present and malformed is a refusal, not a fallback: the
/// alternative is an application that silently ignores the port somebody just
/// set and binds somewhere else.
fn read_file(data_dir: &Path) -> StartResult<AppConfig> {
    let path = data_dir.join(CONFIG_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(defaults());
        }
        Err(error) => {
            return Err(StartError::io(format!("reading {}", path.display()), error));
        }
    };
    let table: toml::Table = toml::from_str(&text).map_err(|error| {
        StartError::Cli(gproxy::Error::config(
            path.display().to_string(),
            error.to_string(),
        ))
    })?;
    // Whether the file *named* a port, rather than what it parsed to.
    // `AppConfig`'s serde default is the server's 7070, so a file that says
    // nothing would otherwise be indistinguishable from one that asks to
    // collide with a running `gproxy serve`.
    let names_port = table.contains_key("port");
    let mut config: AppConfig = table.try_into().map_err(|error: toml::de::Error| {
        StartError::Cli(gproxy::Error::config(
            path.display().to_string(),
            error.to_string(),
        ))
    })?;
    if !names_port {
        config.port = DEFAULT_PORT;
    }
    Ok(config)
}

fn defaults() -> AppConfig {
    AppConfig {
        port: DEFAULT_PORT,
        ..AppConfig::default()
    }
}

/// Where the database file for `data_dir` is, for the caller that has to know
/// whether this instance is new.
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DATABASE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::{MemoryStore, master_key};

    fn key(data_dir: &Path) -> MasterKeyOutcome {
        master_key(&MemoryStore::default(), data_dir, true).unwrap()
    }

    #[test]
    fn the_defaults_are_a_loopback_instance_in_the_data_directory() {
        let data = tempfile::tempdir().unwrap();
        let settings = settings(data.path(), key(data.path())).unwrap();
        assert_eq!(settings.config.host, LOOPBACK);
        assert_eq!(settings.config.port, DEFAULT_PORT);
        assert_eq!(
            settings.config.data_dir.as_deref(),
            Some(data.path().to_string_lossy().as_ref())
        );
        assert_eq!(
            settings.config.store,
            StoreBackendConfig::Sqlite {
                path: DATABASE_FILE.into()
            }
        );
        assert!(!settings.config.console.enabled);
        assert!(settings.config.file_storage.is_some());
        assert_eq!(settings.admin.user, DESKTOP_ADMIN_USER);
        assert!(settings.admin.password.is_none());
    }

    #[test]
    fn the_file_can_move_the_port_and_leave_everything_else_alone() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(CONFIG_FILE), "port = 9099\n").unwrap();
        let settings = settings(data.path(), key(data.path())).unwrap();
        assert_eq!(settings.config.port, 9099);
        assert_eq!(settings.config.host, LOOPBACK);
    }

    #[test]
    fn a_file_that_says_nothing_about_the_port_does_not_collide_with_the_server() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(CONFIG_FILE), "session_ttl_secs = 60\n").unwrap();
        let settings = settings(data.path(), key(data.path())).unwrap();
        assert_eq!(settings.config.port, DEFAULT_PORT);
        assert_eq!(settings.config.session_ttl_secs, 60);
    }

    #[test]
    fn the_file_cannot_move_the_database_out_of_the_data_directory() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(
            data.path().join(CONFIG_FILE),
            "[store]\nkind = \"url\"\ndsn = \"postgres://elsewhere/gproxy\"\n",
        )
        .unwrap();
        let settings = settings(data.path(), key(data.path())).unwrap();
        assert_eq!(
            settings.config.store,
            StoreBackendConfig::Sqlite {
                path: DATABASE_FILE.into()
            }
        );
    }

    #[test]
    fn the_file_cannot_move_the_listener_off_loopback() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(CONFIG_FILE), "host = \"0.0.0.0\"\n").unwrap();
        let settings = settings(data.path(), key(data.path())).unwrap();
        assert_eq!(settings.config.host, LOOPBACK);
    }

    #[test]
    fn a_malformed_file_is_refused_rather_than_ignored() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(CONFIG_FILE), "port = \"seventy\"\n").unwrap();
        let error = settings(data.path(), key(data.path())).unwrap_err();
        assert!(error.to_string().contains(CONFIG_FILE), "{error}");
    }
}
