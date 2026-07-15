//! Extract the executable (and any required runtime companions) from a
//! downloaded release `.zip` (§19.5) — NATIVE only.
//!
//! The release packager ships each platform as a `.zip` (binary + README). The
//! self-update artifact `url`/`sha256` therefore point at the `.zip`; after the
//! zip's bytes are sha256-checked and the manifest signature is verified, the
//! executable is pulled out here and handed to [`super::swap::install`]. The
//! extracted binary inherits the zip's verified trust — no separate inner hash.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use super::UpdateError;

#[derive(Debug)]
pub struct StagedBinary {
    pub executable: PathBuf,
    pub companions: Vec<StagedCompanion>,
}

#[derive(Debug)]
pub struct StagedCompanion {
    pub path: PathBuf,
    pub file_name: &'static str,
}

/// Extract the executable selected for `target` to a sibling `<stem>.bin`
/// file. Android portable archives contain both a shell launcher (`gproxy`)
/// and the real executable (`gproxy.bin`), so they must prefer the latter and
/// carry their C++ runtime forward with the update.
pub fn extract_binary(zip_path: &Path, target: &str) -> Result<StagedBinary, UpdateError> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| UpdateError::Integrity(format!("update artifact is not a valid zip: {e}")))?;

    let android_portable = matches!(target, "x86_64-linux-android" | "aarch64-linux-android");
    let preferred_names: &[&str] = if android_portable {
        &["gproxy.bin", "gproxy"]
    } else {
        &["gproxy", "gproxy.exe"]
    };
    let idx = named_entry_index(&mut archive, preferred_names).ok_or_else(|| {
        UpdateError::Integrity("update artifact zip contains no gproxy executable".to_string())
    })?;
    let executable = zip_path.with_extension("bin");
    extract_entry(&mut archive, idx, &executable)?;

    let mut companions = Vec::new();
    if android_portable {
        let file_name = "libc++_shared.so";
        let idx = named_entry_index(&mut archive, &[file_name]).ok_or_else(|| {
            UpdateError::Integrity(format!("Android update artifact contains no {file_name}"))
        })?;
        let path = zip_path.with_extension("libcxx.so");
        extract_entry(&mut archive, idx, &path)?;
        companions.push(StagedCompanion { path, file_name });
    }

    Ok(StagedBinary {
        executable,
        companions,
    })
}

fn extract_entry(
    archive: &mut ZipArchive<File>,
    idx: usize,
    out: &Path,
) -> Result<(), UpdateError> {
    let mut entry = archive
        .by_index(idx)
        .map_err(|e| UpdateError::Integrity(format!("reading update zip entry: {e}")))?;
    let mut out_file = File::create(out)?;
    io::copy(&mut entry, &mut out_file)?;
    Ok(())
}

/// Locate by preferred basename order. `enclosed_name` rejects traversal
/// entries before their basenames are considered.
fn named_entry_index(archive: &mut ZipArchive<File>, names: &[&str]) -> Option<usize> {
    for name in names {
        if let Some(idx) = (0..archive.len()).find(|&i| {
            archive
                .by_index(i)
                .ok()
                .and_then(|e| {
                    e.enclosed_name()
                        .and_then(|p| p.file_name().map(|f| f.to_owned()))
                })
                .is_some_and(|f| f == *name)
        }) {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        for (name, data) in entries {
            zw.start_file(*name, SimpleFileOptions::default()).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn extracts_gproxy_ignoring_readme() {
        let dir = std::env::temp_dir().join("gproxy-extract-test");
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("artifact.tmp");
        write_zip(
            &zip_path,
            &[("README.md", b"hello"), ("gproxy", b"\x7fELF-binary-bytes")],
        );

        let out = extract_binary(&zip_path, "x86_64-unknown-linux-gnu").expect("extract");
        assert_eq!(out.executable, zip_path.with_extension("bin"));
        assert!(out.companions.is_empty());
        assert_eq!(
            std::fs::read(&out.executable).unwrap(),
            b"\x7fELF-binary-bytes"
        );

        let _ = std::fs::remove_file(&zip_path);
        let _ = std::fs::remove_file(&out.executable);
    }

    #[test]
    fn android_extracts_real_binary_and_shared_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("android.tmp");
        write_zip(
            &zip_path,
            &[
                ("gproxy", b"#!/system/bin/sh"),
                ("gproxy.bin", b"\x7fELF-android"),
                ("libc++_shared.so", b"\x7fELF-libcxx"),
            ],
        );

        let out = extract_binary(&zip_path, "aarch64-linux-android").expect("extract");
        assert_eq!(std::fs::read(&out.executable).unwrap(), b"\x7fELF-android");
        assert_eq!(out.companions.len(), 1);
        assert_eq!(out.companions[0].file_name, "libc++_shared.so");
        assert_eq!(
            std::fs::read(&out.companions[0].path).unwrap(),
            b"\x7fELF-libcxx"
        );
    }

    #[test]
    fn missing_binary_is_integrity_error() {
        let dir = std::env::temp_dir().join("gproxy-extract-test");
        std::fs::create_dir_all(&dir).unwrap();
        let zip_path = dir.join("no-bin.tmp");
        write_zip(&zip_path, &[("README.md", b"only docs")]);

        let err = extract_binary(&zip_path, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(matches!(err, UpdateError::Integrity(_)));
        let _ = std::fs::remove_file(&zip_path);
    }
}
