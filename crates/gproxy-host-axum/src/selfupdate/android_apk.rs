use std::path::{Path, PathBuf};

use super::Result;

pub(super) fn stage(data_dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let directory = data_dir.join(".update");
    std::fs::create_dir_all(&directory)?;
    let temporary = directory.join("gproxy-update.apk.tmp");
    let destination = directory.join("gproxy-update.apk");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, &destination)?;
    std::fs::write(directory.join("install-apk.pending"), b"1\n")?;
    Ok(destination)
}
