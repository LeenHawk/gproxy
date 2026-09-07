use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use super::*;
use crate::android::Runtime;

pub(super) fn status(_manager: &Manager) -> Status {
    match crate::android::runtime() {
        // The packaged app owns startup: its own switch drives a
        // BOOT_COMPLETED receiver that the binary cannot reach.
        Runtime::App => unsupported("app"),
        Runtime::Bare => unsupported("termux"),
        Runtime::Termux { home, .. } => Status {
            supported: true,
            enabled: termux::boot_script(&home).exists(),
            platform: "termux".into(),
            detail: Some("termux-boot".into()),
        },
    }
}

pub(super) fn set_enabled(manager: &Manager, enabled: bool) -> Result<(), Error> {
    let Runtime::Termux { home, shell } = crate::android::runtime() else {
        return Err(Error::Unsupported);
    };
    let path = termux::boot_script(&home);
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&manager.data_dir)?;
    let log = manager.data_dir.join("autostart.log");
    // Restrict access before writing arguments that can contain --master-key.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o700)
        .open(&path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    file.set_len(0)?;
    file.write_all(termux::script(manager, &shell, &log).as_bytes())?;
    Ok(())
}

fn unsupported(detail: &str) -> Status {
    Status {
        supported: false,
        enabled: false,
        platform: "android".into(),
        detail: Some(detail.into()),
    }
}
