use std::io::Cursor;
use std::path::{Path, PathBuf};

use super::{Error, Result};

pub(super) fn binary(archive: &[u8], directory: &Path, target: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    restrict(directory)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(|_| Error::Archive)?;
    let preferred: &[&str] = if target.contains("android") {
        &["gproxy.bin", "gproxy"]
    } else {
        &["gproxy", "gproxy.exe"]
    };
    let index = preferred
        .iter()
        .find_map(|name| find(&mut zip, name))
        .ok_or(Error::Archive)?;
    let mut entry = zip.by_index(index).map_err(|_| Error::Archive)?;
    let path = directory.join("gproxy.staged");
    let mut output = std::fs::File::create(&path)?;
    std::io::copy(&mut entry, &mut output)?;
    Ok(path)
}

fn find(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, expected: &str) -> Option<usize> {
    (0..zip.len()).find(|index| {
        zip.by_index(*index)
            .ok()
            .and_then(|entry| {
                entry
                    .enclosed_name()
                    .and_then(|path| path.file_name().map(ToOwned::to_owned))
            })
            .is_some_and(|name| name == expected)
    })
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<()> {
    Ok(())
}
