//! Optional fonts. Reads never access the network; only `download` does.
use std::{collections::BTreeMap, io, path::PathBuf, sync::Mutex, time::Duration};

use futures_util::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Public npm CDNs, tried in order for each file. Every font is a file of a
/// pinned Fontsource package, and the hash check below is what makes any of
/// them trustworthy. npmmirror first: it is the one reachable from mainland
/// China, and it is fast everywhere else too.
const MIRRORS: [fn(&str, &str, &str) -> String; 3] = [
    |package, version, path| {
        format!("https://registry.npmmirror.com/{package}/{version}/files/{path}")
    },
    |package, version, path| format!("https://cdn.jsdelivr.net/npm/{package}@{version}/{path}"),
    |package, version, path| format!("https://unpkg.com/{package}@{version}/{path}"),
];

#[derive(Deserialize)]
pub(super) struct FontPackage {
    pub stylesheet: String,
    /// npm package name to the exact version the stylesheet was built from.
    packages: BTreeMap<String, String>,
    files: Vec<FontFile>,
}

/// One font file: the content-addressed name the stylesheet uses, and where
/// that content lives in its npm package.
#[derive(Clone, Deserialize)]
struct FontFile {
    name: String,
    package: String,
    path: String,
}

impl FontFile {
    /// The source is spliced into CDN URLs, so it is held to exactly the shape
    /// Fontsource publishes: a scoped package and a plain `files/` path.
    fn is_valid(&self, packages: &BTreeMap<String, String>) -> bool {
        let plain = |value: &str| {
            !value.is_empty()
                && value != ".."
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        filename(&format!("fonts/{}", self.name)).is_some()
            && packages
                .get(&self.package)
                .is_some_and(|version| plain(version))
            && self
                .package
                .strip_prefix("@fontsource-variable/")
                .is_some_and(plain)
            && self.path.strip_prefix("files/").is_some_and(plain)
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "documentation", derive(schemars::JsonSchema))]
pub struct FontStatus {
    pub installed: bool,
    pub downloading: bool,
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug)]
pub struct FontCache {
    root: PathBuf,
    gate: tokio::sync::Mutex<()>,
    progress: Mutex<Option<(usize, usize)>>,
}

struct Download<'a>(&'a Mutex<Option<(usize, usize)>>);
impl Drop for Download<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = None;
    }
}

impl FontCache {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            gate: tokio::sync::Mutex::new(()),
            progress: Mutex::new(None),
        }
    }

    pub async fn status(&self) -> FontStatus {
        let installed = tokio::fs::try_exists(self.root.join("active.css"))
            .await
            .unwrap_or(false);
        let progress = *self.progress.lock().unwrap();
        let (completed, total) = progress.unwrap_or_default();
        FontStatus {
            installed,
            downloading: progress.is_some(),
            completed,
            total,
        }
    }

    pub(super) async fn stylesheet(&self) -> Option<Vec<u8>> {
        tokio::fs::read(self.root.join("active.css")).await.ok()
    }

    pub(super) async fn read(&self, filename: &str) -> Option<Vec<u8>> {
        self.stylesheet().await.as_ref()?;
        self.cached(filename).await
    }

    async fn cached(&self, filename: &str) -> Option<Vec<u8>> {
        let bytes = tokio::fs::read(self.root.join(filename)).await.ok()?;
        matches_hash(filename, &bytes).then_some(bytes)
    }

    pub async fn remove(&self) -> io::Result<FontStatus> {
        let _gate = self
            .gate
            .try_lock()
            .map_err(|_| io::Error::other("Font download is in progress"))?;
        match tokio::fs::remove_dir_all(&self.root).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(self.status().await)
    }

    pub(super) async fn download(
        &self,
        package: &FontPackage,
        css: &[u8],
    ) -> io::Result<FontStatus> {
        let _gate = self
            .gate
            .try_lock()
            .map_err(|_| io::Error::other("Font download is in progress"))?;
        if package
            .files
            .iter()
            .any(|file| !file.is_valid(&package.packages))
        {
            return Err(io::Error::other("Invalid font manifest"));
        }
        *self.progress.lock().unwrap() = Some((0, package.files.len()));
        let progress = Download(&self.progress);
        let client = reqwest::Client::builder()
            .user_agent(concat!("gproxy-fonts/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(io::Error::other)?;
        tokio::fs::create_dir_all(&self.root).await?;
        stream::iter(package.files.clone().into_iter().map(|file| {
            let client = client.clone();
            let version = package.packages[&file.package].clone();
            async move {
                if self.cached(&file.name).await.is_none() {
                    let bytes = fetch(&client, &file, &version).await?;
                    let pending = self.root.join(format!("{}.part", file.name));
                    tokio::fs::write(&pending, bytes).await?;
                    tokio::fs::rename(pending, self.root.join(&file.name)).await?;
                }
                if let Some((completed, _)) = self.progress.lock().unwrap().as_mut() {
                    *completed += 1;
                }
                Ok::<(), io::Error>(())
            }
        }))
        .buffer_unordered(6)
        .try_collect::<Vec<_>>()
        .await?;
        // Only a complete, explicitly requested font pack becomes active.
        let pending = self.root.join("active.css.part");
        tokio::fs::write(&pending, css).await?;
        tokio::fs::rename(pending, self.root.join("active.css")).await?;
        drop(progress);
        Ok(self.status().await)
    }
}

/// The first mirror that returns the right bytes. A mirror that is down, slow
/// or serves something else is skipped; the last failure is what is reported.
async fn fetch(client: &reqwest::Client, file: &FontFile, version: &str) -> io::Result<Vec<u8>> {
    let mut failure = io::Error::other("no font mirror configured");
    for mirror in MIRRORS {
        let url = mirror(&file.package, version, &file.path);
        let response = client
            .get(&url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status);
        let bytes = match response {
            Ok(response) => response.bytes().await,
            Err(error) => Err(error),
        };
        match bytes {
            Ok(bytes) if matches_hash(&file.name, &bytes) => return Ok(bytes.to_vec()),
            Ok(_) => {
                failure = io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Font checksum mismatch from {url}"),
                );
            }
            Err(error) => failure = io::Error::other(error),
        }
    }
    Err(failure)
}

pub(super) fn filename(asset: &str) -> Option<&str> {
    let filename = asset.strip_prefix("fonts/")?;
    let hash = filename.strip_suffix(".woff2")?;
    (hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(filename)
}

fn matches_hash(filename: &str, bytes: &[u8]) -> bool {
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    filename.strip_suffix(".woff2") == Some(hash.as_str())
}
