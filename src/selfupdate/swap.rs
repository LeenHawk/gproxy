//! Binary swap + restart (§19.5 / §19.6 / §19.8) — NATIVE only.
//!
//! This is the risky I/O seam: it touches the running executable and the
//! process lifecycle, so it is exercised only by a real release, never by a
//! unit test. `self-replace` smooths over the Unix vs Windows difference (a
//! running Unix process keeps its old inode; a Windows `.exe` can't be
//! overwritten in place). We retain `<exe>.prev` for rollback (§19.8).

use std::path::{Path, PathBuf};

use super::UpdateError;
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

/// Atomically install the staged binary over the running executable, keeping a
/// `<exe>.prev` copy for rollback (§19.5 / §19.8).
///
/// Steps: mark the staged file executable → copy the current binary to
/// `<exe>.prev` → `self_replace::self_replace` (atomic swap, Unix/Windows aware).
pub fn install(staged: &Path) -> Result<(), UpdateError> {
    make_executable(staged)?;

    let exe = current_exe_path()?;
    let prev = prev_path(&exe);
    // Best-effort rollback copy; failure here is fatal (we won't proceed without
    // a rollback path).
    std::fs::copy(&exe, &prev).map_err(|e| {
        UpdateError::Swap(format!("failed to retain rollback copy at {prev:?}: {e}"))
    })?;

    self_replace::self_replace(staged)
        .map_err(|e| UpdateError::Swap(format!("self_replace failed: {e}")))?;

    // The staged temp file is consumed by self_replace on success; clean up any
    // residue defensively.
    let _ = std::fs::remove_file(staged);
    Ok(())
}

fn prev_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".prev");
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
    fn prev_path_appends_suffix() {
        let p = prev_path(Path::new("/usr/local/bin/gproxy"));
        assert_eq!(p, PathBuf::from("/usr/local/bin/gproxy.prev"));
    }
}
