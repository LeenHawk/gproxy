//! `.env`, loaded into the process environment before anything reads it.
//!
//! # Why this runs before the runtime exists
//!
//! Loading a `.env` means calling [`std::env::set_var`], which is unsound while
//! another thread may be reading the environment. So this is called from `main`
//! before the Tokio runtime is built and before `clap` parses — at that point
//! the process is single-threaded and the write is safe. Every later reader,
//! `clap` included, sees the merged result.
//!
//! # Why it never overrides
//!
//! A real environment variable is a deliberate act by whoever started the
//! process: a `docker run -e`, a systemd `Environment=`, an operator typing it.
//! A `.env` is a file that was checked out. When the two disagree the person
//! present wins, which is why nothing already set is touched.
//!
//! # What is loaded
//!
//! Every key in the file, not only `GPROXY_*`. v3 filtered to its own prefix on
//! the grounds that a shared `.env` carries unrelated tokens; that filter is
//! dropped because it also silently swallowed `RUST_LOG` and anything a future
//! variable might be called, and because "this file configures this process" is
//! what every other tool in a deployment already assumes of `.env`.

use std::path::{Path, PathBuf};

use crate::{Error, Result, cli};

/// The file consulted when nothing names another.
pub const DEFAULT_ENV_FILE: &str = ".env";

/// The path to load: `GPROXY_ENV_FILE` if the real environment names one, else
/// `./.env`.
///
/// Read from the environment rather than from a flag on purpose. A flag would
/// have to be parsed by `clap`, and `clap` is the step this one feeds — a
/// `--env-file` could not affect the environment `clap` itself reads, so it
/// would work for everything except the values an operator would most expect it
/// to cover.
pub fn path() -> PathBuf {
    std::env::var_os(cli::ENV_FILE)
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from(DEFAULT_ENV_FILE), PathBuf::from)
}

/// Load [`path`]. A missing file is not an error: `.env` is a convenience, and
/// most deployments set the environment directly.
///
/// # Safety
///
/// Call once, from `main`, before any thread is spawned.
pub fn load_default() -> Result<()> {
    let path = path();
    let named = std::env::var_os(cli::ENV_FILE).is_some_and(|value| !value.is_empty());
    match load(&path) {
        Ok(()) => Ok(()),
        // A file the operator asked for by name and that is not there is a
        // mistake worth reporting; the implicit `./.env` is not.
        Err(Error::Io { error, .. }) if error.kind() == std::io::ErrorKind::NotFound && !named => {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Merge one `.env` into the process environment, leaving every key that is
/// already set alone.
///
/// # Safety
///
/// Same rule as [`load_default`]: single-threaded, before the runtime.
pub fn load(path: &Path) -> Result<()> {
    // `from_path` is `dotenvy`'s non-overriding load, which is exactly the
    // precedence this module documents: the real environment wins.
    dotenvy::from_path(path).map_err(|error| match error {
        dotenvy::Error::Io(error) => Error::io(format!("reading {}", path.display()), error),
        other => Error::config(
            format!("{} / {}", path.display(), cli::ENV_FILE),
            other.to_string(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_implicit_file_is_not_a_failure() {
        let missing = Path::new("/nonexistent/gproxy/.env");
        let error = load(missing).unwrap_err();
        assert!(matches!(
            error,
            Error::Io { ref error, .. } if error.kind() == std::io::ErrorKind::NotFound
        ));
    }
}
