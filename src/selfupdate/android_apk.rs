//! Android APK hand-off after the release artifact has passed manifest
//! signature and SHA-256 verification.

use std::path::{Path, PathBuf};

use super::UpdateError;

pub const APK_FILE_NAME: &str = "gproxy-update.apk";
pub const APK_READY_FILE_NAME: &str = "install-apk.pending";

/// Move the verified APK to the fixed private path exposed by the APK's
/// read-only content provider, then atomically publish a marker for the Java
/// foreground service. The service opens Android's package installer after
/// this process exits with the update sentinel.
pub fn stage(data_dir: &Path, downloaded: &Path) -> Result<PathBuf, UpdateError> {
    let update_dir = data_dir.join(".update");
    std::fs::create_dir_all(&update_dir)?;
    let apk = update_dir.join(APK_FILE_NAME);
    if apk.exists() {
        std::fs::remove_file(&apk)?;
    }
    std::fs::rename(downloaded, &apk)?;

    let marker = update_dir.join(APK_READY_FILE_NAME);
    let marker_tmp = update_dir.join(format!("{APK_READY_FILE_NAME}.tmp"));
    std::fs::write(&marker_tmp, b"ready\n")?;
    std::fs::rename(marker_tmp, marker)?;
    Ok(apk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_apk_and_publishes_marker() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let downloaded = dir.path().join("download.tmp");
        std::fs::write(&downloaded, b"apk").unwrap();

        let apk = stage(&data_dir, &downloaded).unwrap();
        assert_eq!(apk, data_dir.join(".update").join(APK_FILE_NAME));
        assert_eq!(std::fs::read(apk).unwrap(), b"apk");
        assert!(data_dir.join(".update").join(APK_READY_FILE_NAME).is_file());
        assert!(!downloaded.exists());
    }
}
