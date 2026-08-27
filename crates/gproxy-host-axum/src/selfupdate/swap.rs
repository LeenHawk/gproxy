use std::path::{Path, PathBuf};

use super::{Error, Result};

pub(super) fn install(staged: &Path) -> Result<()> {
    let executable = std::env::current_exe()?;
    install_at(&executable, staged, false)
}

pub(super) fn rollback_available() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|path| previous(&path).is_file())
}

pub(super) fn rollback() -> Result<()> {
    let executable = std::env::current_exe()?;
    rollback_at(&executable)
}

fn rollback_at(executable: &Path) -> Result<()> {
    let previous = previous(executable);
    if !previous.is_file() {
        return Err(Error::Rollback);
    }
    let current = appended(executable, ".rollback-current");
    std::fs::copy(executable, &current)?;
    replace_running(executable, &previous).inspect_err(|_| {
        let _ = std::fs::remove_file(&current);
    })?;
    std::fs::rename(current, previous)?;
    Ok(())
}

fn install_at(target: &Path, staged: &Path, fail_after_backup: bool) -> Result<()> {
    let previous = previous(target);
    std::fs::copy(target, &previous)?;
    let temporary = appended(target, ".update");
    std::fs::copy(staged, &temporary)?;
    make_executable(&temporary)?;
    if fail_after_backup {
        restore(target, &previous)?;
        let _ = std::fs::remove_file(temporary);
        return Err(Error::Swap);
    }
    replace_running(target, &temporary).inspect_err(|_| {
        let _ = restore(target, &previous);
        let _ = std::fs::remove_file(&temporary);
    })
}

#[cfg(unix)]
fn replace_running(target: &Path, replacement: &Path) -> Result<()> {
    std::fs::rename(replacement, target).map_err(Into::into)
}

#[cfg(not(unix))]
fn replace_running(_target: &Path, replacement: &Path) -> Result<()> {
    self_replace::self_replace(replacement).map_err(|_| Error::Swap)
}

fn restore(target: &Path, previous: &Path) -> Result<()> {
    std::fs::copy(previous, target)?;
    make_executable(target)
}

fn previous(path: &Path) -> PathBuf {
    appended(path, ".prev")
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_swap_restores_and_successful_swap_can_roll_back() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("gproxy");
        let staged = directory.path().join("staged");
        std::fs::write(&target, b"working").unwrap();
        std::fs::write(&staged, b"broken").unwrap();

        assert!(super::install_at(&target, &staged, true).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"working");
        assert_eq!(std::fs::read(super::previous(&target)).unwrap(), b"working");

        super::install_at(&target, &staged, false).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"broken");
        super::rollback_at(&target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"working");
        assert_eq!(std::fs::read(super::previous(&target)).unwrap(), b"broken");
    }
}
