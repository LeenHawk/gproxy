//! Replacing the running executable, and putting it back.
//!
//! # Why a rename and not a write
//!
//! A running executable cannot be opened for writing on Unix — the kernel
//! holds the text pages and answers `ETXTBSY` — but it *can* be renamed out
//! from under itself, because the running process holds an inode and a rename
//! only moves a directory entry. So the new binary is written to a sibling
//! path, made executable, and renamed over the old name in one atomic step.
//! There is no window in which the name points at a half-written file.
//!
//! Windows will not rename over an open executable either, and has no
//! equivalent trick, so `self_replace` does the platform dance there.
//!
//! # The backup is taken before anything is replaced
//!
//! `gproxy.prev` is a copy of the executable that was running, taken first.
//! It is what [`rollback`] restores and what [`install_at`] restores if the
//! rename fails — a gateway that cannot be started is worse than one that is a
//! release behind, and the failure mode a swap has to be safe against is
//! exactly "the new file is there and the old one is gone".
//!
//! # What this module does not do
//!
//! It does not verify anything. Everything it touches has already been through
//! the manifest's signature and the artifact's hash, and re-deciding that here
//! would be a second opinion that could disagree with the first.

use std::path::{Path, PathBuf};

use super::config::UpdateError;

/// Replace the executable this process is running with `staged`.
pub(super) fn install(staged: &Path) -> Result<(), UpdateError> {
    let executable = current_exe()?;
    install_at(&executable, staged, false)
}

/// Whether there is a previous executable to go back to.
pub(super) fn rollback_available() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|path| previous(&path).is_file())
}

/// Put the previous executable back, keeping the one being replaced as the new
/// rollback target — so a rollback can itself be rolled back.
pub(super) fn rollback() -> Result<(), UpdateError> {
    let executable = current_exe()?;
    rollback_at(&executable)
}

fn current_exe() -> Result<PathBuf, UpdateError> {
    std::env::current_exe().map_err(|error| UpdateError::io("locating this executable", error))
}

fn rollback_at(executable: &Path) -> Result<(), UpdateError> {
    let previous = previous(executable);
    if !previous.is_file() {
        return Err(UpdateError::Rollback);
    }
    // Keep what is running now, so the swap is reversible in both directions.
    let current = appended(executable, ".rollback-current");
    copy(executable, &current)?;
    replace_running(executable, &previous).inspect_err(|_| {
        let _ = std::fs::remove_file(&current);
    })?;
    std::fs::rename(&current, &previous)
        .map_err(|error| UpdateError::io(format!("renaming {}", current.display()), error))
}

/// The whole swap, with an injectable failure.
///
/// `fail_after_backup` is the one thing a test cannot arrange from outside:
/// the interesting failure is a rename that fails *after* the backup exists,
/// and there is no portable way to make `rename(2)` fail on demand. So the
/// branch is a parameter, and the restore path it exercises is the same code
/// the real failure takes.
fn install_at(target: &Path, staged: &Path, fail_after_backup: bool) -> Result<(), UpdateError> {
    let previous = previous(target);
    copy(target, &previous)?;
    let temporary = appended(target, ".update");
    copy(staged, &temporary)?;
    make_executable(&temporary)?;
    if fail_after_backup {
        restore(target, &previous)?;
        let _ = std::fs::remove_file(&temporary);
        return Err(UpdateError::Swap);
    }
    replace_running(target, &temporary).inspect_err(|_| {
        let _ = restore(target, &previous);
        let _ = std::fs::remove_file(&temporary);
    })
}

#[cfg(unix)]
fn replace_running(target: &Path, replacement: &Path) -> Result<(), UpdateError> {
    std::fs::rename(replacement, target)
        .map_err(|error| UpdateError::io(format!("replacing {}", target.display()), error))
}

#[cfg(not(unix))]
fn replace_running(_target: &Path, replacement: &Path) -> Result<(), UpdateError> {
    self_replace::self_replace(replacement).map_err(|_| UpdateError::Swap)
}

fn restore(target: &Path, previous: &Path) -> Result<(), UpdateError> {
    copy(previous, target)?;
    make_executable(target)
}

fn copy(from: &Path, to: &Path) -> Result<(), UpdateError> {
    std::fs::copy(from, to).map(|_| ()).map_err(|error| {
        UpdateError::io(
            format!("copying {} to {}", from.display(), to.display()),
            error,
        )
    })
}

/// `…/gproxy.prev`: the executable that was running before the last swap.
fn previous(path: &Path) -> PathBuf {
    appended(path, ".prev")
}

/// A suffix on the whole file name, not on its extension: `gproxy.exe`
/// becomes `gproxy.exe.prev` rather than `gproxy.prev`, so the two platforms'
/// names do not collide.
fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| UpdateError::io(format!("making {} executable", path.display()), error))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UpdateError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_swap_leaves_a_working_executable_where_it_was() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("gproxy");
        let staged = directory.path().join("staged");
        std::fs::write(&target, b"working").unwrap();
        std::fs::write(&staged, b"broken").unwrap();

        let error = install_at(&target, &staged, true).unwrap_err();
        assert!(matches!(error, UpdateError::Swap), "{error}");
        // The one property that matters: the name still points at something
        // that runs.
        assert_eq!(std::fs::read(&target).unwrap(), b"working");
        // And no half-installed sibling was left behind.
        assert!(!appended(&target, ".update").exists());
    }

    #[test]
    fn a_successful_swap_can_be_rolled_back_and_rolled_forward_again() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("gproxy");
        let staged = directory.path().join("staged");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();

        install_at(&target, &staged, false).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(std::fs::read(previous(&target)).unwrap(), b"old");

        rollback_at(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        // The rollback target is now the version that was rolled back from,
        // so the operator is not stuck one way.
        assert_eq!(std::fs::read(previous(&target)).unwrap(), b"new");

        rollback_at(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn a_rollback_with_nothing_to_go_back_to_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("gproxy");
        std::fs::write(&target, b"only ever version").unwrap();
        let error = rollback_at(&target).unwrap_err();
        assert!(matches!(error, UpdateError::Rollback), "{error}");
        // And nothing was touched on the way to finding out.
        assert_eq!(std::fs::read(&target).unwrap(), b"only ever version");
    }

    #[cfg(unix)]
    #[test]
    fn the_installed_executable_is_executable() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("gproxy");
        let staged = directory.path().join("staged");
        std::fs::write(&target, b"old").unwrap();
        // Staged out of a zip entry, which carries no mode worth trusting.
        std::fs::write(&staged, b"new").unwrap();
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).unwrap();

        install_at(&target, &staged, false).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "{mode:o}");
    }

    #[test]
    fn the_suffix_lands_on_the_whole_name_so_the_two_platforms_do_not_collide() {
        assert_eq!(
            previous(Path::new("/opt/gproxy/gproxy.exe")),
            PathBuf::from("/opt/gproxy/gproxy.exe.prev")
        );
        assert_eq!(
            previous(Path::new("/usr/local/bin/gproxy")),
            PathBuf::from("/usr/local/bin/gproxy.prev")
        );
    }
}
