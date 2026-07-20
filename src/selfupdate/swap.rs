//! Binary swap + restart (§19.5 / §19.6 / §19.8) — NATIVE only.
//!
//! This is the risky I/O seam: it touches the running executable and the
//! process lifecycle, so it is exercised only by a real release, never by a
//! unit test. `self-replace` smooths over the Unix vs Windows difference (a
//! running Unix process keeps its old inode; a Windows `.exe` can't be
//! overwritten in place). Temporary companion backups exist only for the
//! duration of the swap and are removed on success.

use std::path::{Path, PathBuf};

use super::UpdateError;
use super::extract::{StagedBinary, StagedCompanion};
use super::verify::sha256_hex;

/// Sentinel exit code the supervisor model exits with after staging a new
/// binary (§19.6.1). Distinct from a crash so a supervisor/orchestrator can be
/// configured to treat it as an intentional restart.
pub const RESTART_SENTINEL_CODE: i32 = 42;

/// sha256 (lowercase hex) of the currently-running executable. Used by the
/// `staging` channel to detect a rolling re-upload (§19.3). The caller is
/// expected to compute this once at startup and cache it; here we read on
/// demand (cheap relative to a network round-trip).
pub fn current_exe_sha256() -> Result<String, UpdateError> {
    let exe = std::env::current_exe()?;
    let bytes = std::fs::read(&exe)?;
    Ok(sha256_hex(&bytes))
}

/// Path of the running executable.
fn current_exe_path() -> Result<PathBuf, UpdateError> {
    std::env::current_exe().map_err(UpdateError::Io)
}

/// Atomically install the staged binary over the running executable (§19.5).
///
/// Steps: mark the staged file executable → install runtime companions →
/// `self_replace::self_replace` (atomic swap, Unix/Windows aware).
pub fn install(staged: &StagedBinary) -> Result<(), UpdateError> {
    make_executable(&staged.executable)?;

    let exe = current_exe_path()?;
    let installed_companions = install_companions(&exe, &staged.companions)?;
    if let Err(error) = self_replace::self_replace(&staged.executable) {
        restore_companions(&installed_companions);
        return Err(UpdateError::Swap(format!("self_replace failed: {error}")));
    }
    finish_companions(&installed_companions);
    remove_legacy_prev(&exe);

    // The staged temp file is consumed by self_replace on success; clean up any
    // residue defensively.
    let _ = std::fs::remove_file(&staged.executable);
    for companion in &staged.companions {
        let _ = std::fs::remove_file(&companion.path);
    }
    Ok(())
}

#[derive(Debug)]
struct InstalledCompanion {
    destination: PathBuf,
    backup: Option<PathBuf>,
    legacy_prev: PathBuf,
}

fn install_companions(
    exe: &Path,
    companions: &[StagedCompanion],
) -> Result<Vec<InstalledCompanion>, UpdateError> {
    let parent = exe.parent().ok_or_else(|| {
        UpdateError::Swap(format!(
            "running executable has no parent directory: {exe:?}"
        ))
    })?;
    let mut installed = Vec::new();
    for companion in companions {
        let destination = parent.join(companion.file_name);
        let backup = if destination.is_file() {
            let backup = appended_path(&destination, ".update-backup");
            if let Err(error) = remove_if_exists(&backup)
                .and_then(|_| std::fs::rename(&destination, &backup).map_err(UpdateError::Io))
            {
                restore_companions(&installed);
                return Err(UpdateError::Swap(format!(
                    "failed to move runtime companion to temporary backup at {backup:?}: {error}"
                )));
            }
            Some(backup)
        } else {
            None
        };
        installed.push(InstalledCompanion {
            legacy_prev: appended_path(&destination, ".prev"),
            destination: destination.clone(),
            backup,
        });
        let temp = appended_path(&destination, ".update");
        if let Err(error) =
            std::fs::copy(&companion.path, &temp).and_then(|_| std::fs::rename(&temp, &destination))
        {
            let _ = std::fs::remove_file(&temp);
            restore_companions(&installed);
            return Err(UpdateError::Swap(format!(
                "failed to install runtime companion at {destination:?}: {error}"
            )));
        }
    }
    Ok(installed)
}

fn restore_companions(installed: &[InstalledCompanion]) {
    for companion in installed.iter().rev() {
        match &companion.backup {
            Some(backup) => {
                let _ = std::fs::remove_file(&companion.destination);
                let _ = std::fs::rename(backup, &companion.destination);
            }
            None => {
                let _ = std::fs::remove_file(&companion.destination);
            }
        }
    }
}

fn finish_companions(installed: &[InstalledCompanion]) {
    for companion in installed {
        if let Some(backup) = &companion.backup {
            remove_best_effort(backup);
        }
        remove_best_effort(&companion.legacy_prev);
    }
}

fn remove_legacy_prev(path: &Path) {
    remove_best_effort(&appended_path(path, ".prev"));
}

fn remove_if_exists(path: &Path) -> Result<(), UpdateError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Swap(format!(
            "failed to remove stale update file at {path:?}: {error}"
        ))),
    }
}

fn remove_best_effort(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(?path, %error, "failed to remove obsolete update file");
    }
}

fn appended_path(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Exit the process with the restart sentinel so a supervisor restarts the
/// freshly-staged binary (§19.6.1). Diverges. Graceful drain (§16.1) is the
/// caller's responsibility before invoking this.
pub fn exit_for_supervisor() -> ! {
    tracing::info!(
        code = RESTART_SENTINEL_CODE,
        "exiting for supervisor restart"
    );
    std::process::exit(RESTART_SENTINEL_CODE);
}

/// Replace the current process image with the new binary via `execv`
/// (§19.6.2 — bare deploy, no supervisor). Diverges on success; only returns an
/// error if the exec syscall itself fails.
///
/// Listening sockets are NOT inherited here (the new process re-binds); the
/// caller should have drained/stopped the listener first.
#[cfg(unix)]
pub fn reexec() -> ! {
    use std::os::unix::process::CommandExt;
    let exe = match current_exe_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("re-exec aborted: {e}");
            std::process::exit(1);
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    tracing::info!(?exe, "re-exec into new binary");
    // `exec` only returns on failure.
    let err = std::process::Command::new(&exe).args(&args).exec();
    tracing::error!("re-exec failed: {err}");
    std::process::exit(1);
}

/// On non-Unix, fall back to the supervisor model (a running Windows `.exe`
/// cannot `execv`-replace itself).
#[cfg(not(unix))]
pub fn reexec() -> ! {
    tracing::warn!("re-exec is Unix-only; falling back to supervisor exit");
    exit_for_supervisor();
}

/// chmod 0755 on Unix; no-op elsewhere (self_replace handles Windows perms).
#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), UpdateError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_backup_is_restorable_and_removed_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("gproxy.bin");
        let destination = dir.path().join("libc++_shared.so");
        let staged = dir.path().join("new-libcxx.so");
        std::fs::write(&exe, b"exe").unwrap();
        std::fs::write(&destination, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();

        let installed = install_companions(
            &exe,
            &[StagedCompanion {
                path: staged,
                file_name: "libc++_shared.so",
            }],
        )
        .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert_eq!(
            std::fs::read(appended_path(&destination, ".update-backup")).unwrap(),
            b"old"
        );

        restore_companions(&installed);
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");

        let installed = install_companions(
            &exe,
            &[StagedCompanion {
                path: dir.path().join("new-libcxx.so"),
                file_name: "libc++_shared.so",
            }],
        )
        .unwrap();
        std::fs::write(appended_path(&destination, ".prev"), b"legacy").unwrap();
        finish_companions(&installed);
        assert!(!appended_path(&destination, ".update-backup").exists());
        assert!(!appended_path(&destination, ".prev").exists());
    }
}
