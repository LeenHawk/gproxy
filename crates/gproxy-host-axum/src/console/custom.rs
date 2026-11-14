//! A console bundle an administrator uploaded, served in place of the built-in
//! one.
//!
//! The bundle is a zip of a static build (`index.html` plus its assets) that
//! is extracted under `<data_dir>/console/current`. It replaces the built-in
//! console for every document request until it is removed, except for a
//! browser carrying the recovery cookie (see [`super::Console::serve`]), which
//! keeps the built-in console reachable however broken the upload is.
//!
//! An upload is extracted into a staging directory first and only swapped in
//! once every entry has been checked, so a rejected or half-written archive
//! never replaces a working bundle.
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};

/// More entries than any static build has; a bound on directory creation.
const MAX_FILES: usize = 20_000;
/// The extracted size, counted from the bytes actually inflated rather than
/// from what the archive declares.
const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "documentation", derive(schemars::JsonSchema))]
pub struct ConsoleBundleStatus {
    /// An uploaded bundle is installed and serves `/console`.
    pub installed: bool,
    /// The binary carries a built-in console, so the recovery switch works.
    pub builtin: bool,
    /// Unix seconds of the upload, when one is installed.
    pub installed_at: Option<u64>,
    pub files: usize,
    pub bytes: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    installed_at: u64,
    files: usize,
    bytes: u64,
}

#[derive(Debug)]
pub struct CustomBundle {
    root: PathBuf,
    active: AtomicBool,
    gate: tokio::sync::Mutex<()>,
}

impl CustomBundle {
    pub fn new(root: PathBuf) -> Self {
        let active = root.join("current").join("index.html").is_file();
        Self {
            root,
            active: AtomicBool::new(active),
            gate: tokio::sync::Mutex::new(()),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// `asset` has already been checked by `asset_name`, so the join stays
    /// inside the bundle.
    pub(super) async fn read(&self, asset: &str) -> Option<Vec<u8>> {
        if !self.is_active() {
            return None;
        }
        tokio::fs::read(self.root.join("current").join(asset))
            .await
            .ok()
    }

    pub async fn status(&self, builtin: bool) -> ConsoleBundleStatus {
        let meta = match self.is_active() {
            true => tokio::fs::read(self.root.join("meta.json"))
                .await
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Meta>(&bytes).ok()),
            false => None,
        };
        ConsoleBundleStatus {
            installed: self.is_active(),
            builtin,
            installed_at: meta.as_ref().map(|meta| meta.installed_at),
            files: meta.as_ref().map_or(0, |meta| meta.files),
            bytes: meta.as_ref().map_or(0, |meta| meta.bytes),
        }
    }

    /// Extract `archive` and make it the served bundle.
    pub async fn install(&self, archive: bytes::Bytes) -> io::Result<()> {
        let _gate = self
            .gate
            .try_lock()
            .map_err(|_| io::Error::other("Another console upload is in progress"))?;
        let root = self.root.clone();
        let staging = root.join(".staging");
        let meta = tokio::task::spawn_blocking({
            let staging = staging.clone();
            move || {
                remove_dir(&staging)?;
                let result = extract(&archive, &staging);
                if result.is_err() {
                    let _ = remove_dir(&staging);
                }
                result
            }
        })
        .await
        .map_err(io::Error::other)??;

        let current = root.join("current");
        let previous = root.join(".previous");
        remove_dir_async(&previous).await?;
        match tokio::fs::rename(&current, &previous).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Err(error) = tokio::fs::rename(&staging, &current).await {
            // Put the old bundle back rather than leave nothing.
            let _ = tokio::fs::rename(&previous, &current).await;
            return Err(error);
        }
        tokio::fs::write(
            root.join("meta.json"),
            serde_json::to_vec(&meta).map_err(io::Error::other)?,
        )
        .await?;
        self.active.store(true, Ordering::Release);
        remove_dir_async(&previous).await?;
        Ok(())
    }

    /// Remove the uploaded bundle; the built-in console serves again.
    pub async fn remove(&self) -> io::Result<()> {
        let _gate = self
            .gate
            .try_lock()
            .map_err(|_| io::Error::other("A console upload is in progress"))?;
        self.active.store(false, Ordering::Release);
        remove_dir_async(&self.root).await
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

async fn remove_dir_async(path: &Path) -> io::Result<()> {
    match tokio::fs::remove_dir_all(path).await {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// A relative path made only of plain segments, the same rule `asset_name`
/// applies to requests, so everything extracted is also servable.
fn plain_path(name: &str) -> Option<&str> {
    let name = name.strip_prefix("./").unwrap_or(name);
    (!name.is_empty()
        && !name.split('/').any(|segment| {
            segment.is_empty()
                || segment == ".."
                || segment == "."
                || segment.contains('\\')
                || segment.contains(':')
        }))
    .then_some(name)
}

/// The archive's files as bundle paths. A build zipped with its `dist/`
/// folder is accepted too: when there is no root `index.html` but every file
/// shares one top-level directory holding one, that directory is the root.
fn bundle_paths(names: &[String]) -> io::Result<Vec<String>> {
    let has = |path: &str| names.iter().any(|name| name == path);
    if has("index.html") {
        return Ok(names.to_vec());
    }
    let prefix = names
        .first()
        .and_then(|name| name.split_once('/'))
        .map(|(first, _)| format!("{first}/"));
    if let Some(prefix) = prefix
        && names.iter().all(|name| name.starts_with(&prefix))
        && has(&format!("{prefix}index.html"))
    {
        return Ok(names
            .iter()
            .map(|name| name[prefix.len()..].to_owned())
            .collect());
    }
    Err(io::Error::other(
        "The archive has no index.html at its root (or inside a single top-level folder)",
    ))
}

fn extract(archive: &[u8], staging: &Path) -> io::Result<Meta> {
    let mut zip = zip::ZipArchive::new(io::Cursor::new(archive))
        .map_err(|error| io::Error::other(format!("Not a zip archive: {error}")))?;
    if zip.len() > MAX_FILES {
        return Err(io::Error::other(format!(
            "The archive has more than {MAX_FILES} entries"
        )));
    }
    // Pass one: names only, so the root can be chosen before writing.
    let mut entries = Vec::new();
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index).map_err(io::Error::other)?;
        if file.is_dir() {
            continue;
        }
        let raw = file.name().map_err(io::Error::other)?.into_owned();
        if file.is_symlink() {
            return Err(io::Error::other(format!(
                "The archive contains a symbolic link: {raw}"
            )));
        }
        let name = plain_path(&raw)
            .ok_or_else(|| io::Error::other(format!("Unsafe path in the archive: {raw}")))?;
        // macOS Finder metadata is never part of a build.
        if name.starts_with("__MACOSX/") || name.ends_with(".DS_Store") {
            continue;
        }
        entries.push((index, name.to_owned()));
    }
    let names: Vec<String> = entries.iter().map(|(_, name)| name.clone()).collect();
    let paths = bundle_paths(&names)?;

    std::fs::create_dir_all(staging)?;
    let mut total = 0u64;
    for ((index, _), path) in entries.iter().zip(&paths) {
        let mut file = zip.by_index(*index).map_err(io::Error::other)?;
        let target = staging.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)?;
        let budget = MAX_EXTRACTED_BYTES - total;
        let written = io::copy(&mut (&mut file).take(budget + 1), &mut out)?;
        if written > budget {
            return Err(io::Error::other(format!(
                "The archive extracts to more than {} MiB",
                MAX_EXTRACTED_BYTES / 1024 / 1024
            )));
        }
        total += written;
    }
    Ok(Meta {
        installed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs()),
        files: paths.len(),
        bytes: total,
    })
}
