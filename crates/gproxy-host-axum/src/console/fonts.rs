//! Optional fonts. Reads never access the network; only `download` does.
use std::{io, path::PathBuf, sync::Mutex, time::Duration};

use futures_util::{StreamExt, TryStreamExt, stream};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CDN: &str = "https://gproxy.leenhawk.com/fonts";

#[derive(Deserialize)]
pub(super) struct FontPackage {
    pub stylesheet: String,
    files: Vec<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
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
        if self.stylesheet().await.is_none() {
            return None;
        }
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
            .any(|name| filename(&format!("fonts/{name}")).is_none())
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
        stream::iter(package.files.clone().into_iter().map(|name| {
            let client = client.clone();
            async move {
                if self.cached(&name).await.is_none() {
                    let bytes = client
                        .get(format!("{CDN}/{name}"))
                        .send()
                        .await
                        .map_err(io::Error::other)?
                        .error_for_status()
                        .map_err(io::Error::other)?
                        .bytes()
                        .await
                        .map_err(io::Error::other)?;
                    if !matches_hash(&name, &bytes) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Font checksum mismatch",
                        ));
                    }
                    let pending = self.root.join(format!("{name}.part"));
                    tokio::fs::write(&pending, bytes).await?;
                    tokio::fs::rename(pending, self.root.join(name)).await?;
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
