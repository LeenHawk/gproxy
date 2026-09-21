//! Unpacking the verified archive.
//!
//! # Only after the hash
//!
//! This module is called with bytes [`super::download::verify`] already
//! accepted, which is what makes reading a zip out of them safe to do at all:
//! a zip parser is a parser, and one fed arbitrary bytes from the network is
//! an attack surface. Here the bytes are exactly the ones the signed manifest
//! named the hash of.
//!
//! # What is extracted, and what is not
//!
//! One entry: the executable. Named `gproxy` or `gproxy.exe`, matched on the
//! **file name** of [`zip::read::ZipFile::enclosed_name`] — which is
//! `zip`'s own path-traversal check, so an entry called
//! `../../.ssh/authorized_keys` resolves to nothing and is skipped rather than
//! written. Nothing else in the archive is unpacked at all, so a release that
//! ships a README next to the binary does not put one anywhere.
//!
//! The staging directory is `0700` before anything lands in it. The staged
//! file is a future executable of a process that holds upstream credentials,
//! and a world-writable `/var/lib/gproxy/.update` would let anyone who can
//! write there choose what the next restart runs.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use super::config::UpdateError;

/// The names an artifact's executable can have. Windows first only in the
/// `.exe` sense — the search is by exact file name, so the order only decides
/// which wins in an archive that somehow carries both.
const EXECUTABLE_NAMES: &[&str] = &["gproxy", "gproxy.exe"];

/// Unpack the executable into `directory`, returning the staged path.
pub(super) fn binary(archive: &[u8], directory: &Path) -> Result<PathBuf, UpdateError> {
    std::fs::create_dir_all(directory)
        .map_err(|error| UpdateError::io(format!("creating {}", directory.display()), error))?;
    restrict(directory)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).map_err(|_| UpdateError::Archive)?;
    let index = EXECUTABLE_NAMES
        .iter()
        .find_map(|name| find(&mut zip, name))
        .ok_or(UpdateError::Archive)?;
    let mut entry = zip.by_index(index).map_err(|_| UpdateError::Archive)?;
    let path = directory.join("gproxy.staged");
    let mut output = std::fs::File::create(&path)
        .map_err(|error| UpdateError::io(format!("creating {}", path.display()), error))?;
    std::io::copy(&mut entry, &mut output)
        .map_err(|error| UpdateError::io(format!("writing {}", path.display()), error))?;
    Ok(path)
}

fn find(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, expected: &str) -> Option<usize> {
    (0..zip.len()).find(|index| {
        zip.by_index(*index)
            .ok()
            // `enclosed_name` is the traversal check: it returns `None` for an
            // absolute path, for anything with a `..` component, and for a
            // Windows drive prefix. An entry it refuses is an entry this
            // search never sees.
            .and_then(|entry| {
                entry
                    .enclosed_name()
                    .and_then(|path| path.file_name().map(ToOwned::to_owned))
            })
            .is_some_and(|name| name == expected)
    })
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| UpdateError::io(format!("restricting {}", path.display()), error))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), UpdateError> {
    // Windows inherits the parent directory's ACL, and the data directory is
    // where the database already lives: a location an operator has already had
    // to get right.
    Ok(())
}

#[cfg(test)]
pub(super) mod fixture {
    //! A one-entry zip, which is what a release archive is.
    use std::io::Write as _;

    /// An archive holding `name` with `contents`, plus whatever extra entries
    /// the caller names first — for asserting that the search picks the
    /// executable and not the README.
    pub(crate) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executable_is_extracted_and_nothing_else_is() {
        let directory = tempfile::tempdir().unwrap();
        let staging = directory.path().join(".update");
        let archive = fixture::archive(&[
            ("README.md", b"not an executable"),
            ("gproxy", b"the new binary"),
        ]);

        let staged = binary(&archive, &staging).unwrap();
        assert_eq!(std::fs::read(&staged).unwrap(), b"the new binary");
        // One file in the staging directory: the binary. The README went
        // nowhere.
        let written: Vec<_> = std::fs::read_dir(&staging)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(written, vec![std::ffi::OsString::from("gproxy.staged")]);
    }

    #[test]
    fn a_nested_path_is_still_found_by_its_file_name() {
        let directory = tempfile::tempdir().unwrap();
        let archive = fixture::archive(&[("gproxy-4.1.0-linux/gproxy", b"nested")]);
        let staged = binary(&archive, &directory.path().join(".update")).unwrap();
        assert_eq!(std::fs::read(staged).unwrap(), b"nested");
    }

    /// An entry that resolves outside the destination is skipped, so the
    /// archive cannot choose where it lands. `zip`'s `enclosed_name` is what
    /// refuses it; this test is here so a future change of that call is caught.
    #[test]
    fn a_traversing_entry_is_not_an_executable() {
        let directory = tempfile::tempdir().unwrap();
        let archive = fixture::archive(&[("../../gproxy", b"escaped")]);
        let error = binary(&archive, &directory.path().join(".update")).unwrap_err();
        assert!(matches!(error, UpdateError::Archive), "{error}");
    }

    #[test]
    fn an_archive_with_no_executable_in_it_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        for archive in [
            fixture::archive(&[("notes.txt", b"nothing to run")]),
            b"this is not a zip file at all".to_vec(),
            Vec::new(),
        ] {
            let error = binary(&archive, &directory.path().join(".update")).unwrap_err();
            assert!(matches!(error, UpdateError::Archive), "{error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_staging_directory_is_not_readable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().unwrap();
        let staging = directory.path().join(".update");
        binary(&fixture::archive(&[("gproxy", b"x")]), &staging).unwrap();
        let mode = std::fs::metadata(&staging).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{mode:o}");
    }
}
