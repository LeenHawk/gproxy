//! Immutable font files fetched from the documentation site's Cloudflare CDN.
//! The CSS is embedded; WOFF2 files are downloaded only when a page needs them.

use std::{io, path::PathBuf, time::Duration};

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

const CDN: &str = "https://gproxy.leenhawk.com/fonts";

#[derive(Debug)]
pub(super) struct FontCache {
    root: PathBuf,
    source: String,
    // Serialize first downloads, including their atomic rename. A second
    // visitor asking for the same face reuses the first visitor's download.
    client: Mutex<Option<reqwest::Client>>,
}

impl FontCache {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            source: CDN.into(),
            client: Mutex::new(None),
        }
    }

    pub(super) async fn read(&self, filename: &str) -> io::Result<Vec<u8>> {
        if let Some(bytes) = self.cached(filename).await {
            return Ok(bytes);
        }
        let mut client = self.client.lock().await;
        if let Some(bytes) = self.cached(filename).await {
            return Ok(bytes);
        }
        if client.is_none() {
            *client = Some(
                reqwest::Client::builder()
                    .user_agent(concat!("gproxy-fonts/", env!("CARGO_PKG_VERSION")))
                    .connect_timeout(Duration::from_secs(5))
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(io::Error::other)?,
            );
        }
        let bytes = client
            .as_ref()
            .unwrap()
            .get(format!("{}/{filename}", self.source))
            .send()
            .await
            .map_err(io::Error::other)?
            .error_for_status()
            .map_err(io::Error::other)?
            .bytes()
            .await
            .map_err(io::Error::other)?;
        if !matches_hash(filename, &bytes) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "font checksum mismatch",
            ));
        }
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.root.join(filename);
        let pending = path.with_extension("woff2.part");
        tokio::fs::write(&pending, &bytes).await?;
        tokio::fs::rename(pending, path).await?;
        Ok(bytes.to_vec())
    }

    async fn cached(&self, filename: &str) -> Option<Vec<u8>> {
        let bytes = tokio::fs::read(self.root.join(filename)).await.ok()?;
        matches_hash(filename, &bytes).then_some(bytes)
    }
}

pub(super) fn filename(asset: &str) -> Option<&str> {
    let filename = asset.strip_prefix("fonts/")?;
    let hash = filename.strip_suffix(".woff2")?;
    (hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then_some(filename)
}

fn matches_hash(filename: &str, bytes: &[u8]) -> bool {
    filename.strip_suffix(".woff2") == Some(hash(bytes).as_str())
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    const FONT: &[u8] = b"wOF2 test font payload";

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!("gproxy-fonts-{}-{now}", std::process::id())))
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    async fn origin(bad_first: bool) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let router = axum::Router::new().route(
            "/{font}",
            axum::routing::get(move || {
                let attempt = observed.fetch_add(1, Ordering::SeqCst);
                async move {
                    if bad_first && attempt == 0 {
                        b"not a font".as_slice()
                    } else {
                        FONT
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, calls, task)
    }

    #[tokio::test]
    async fn first_use_downloads_once_and_a_new_instance_works_offline() {
        let root = Directory::new();
        let (source, calls, server) = origin(false).await;
        let cache = FontCache {
            source: source.clone(),
            ..FontCache::new(root.0.clone())
        };
        let name = format!("{}.woff2", hash(FONT));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!root.0.exists());
        let (first, second) = tokio::join!(cache.read(&name), cache.read(&name));
        assert_eq!(first.unwrap(), FONT);
        assert_eq!(second.unwrap(), FONT);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(tokio::fs::read(root.0.join(&name)).await.unwrap(), FONT);
        server.abort();
        let _ = server.await;
        let restarted = FontCache {
            source,
            ..FontCache::new(root.0.clone())
        };
        assert_eq!(restarted.read(&name).await.unwrap(), FONT);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn bad_downloads_are_not_cached_and_corrupt_files_are_repaired() {
        let root = Directory::new();
        let (source, calls, server) = origin(true).await;
        let cache = FontCache {
            source,
            ..FontCache::new(root.0.clone())
        };
        let name = format!("{}.woff2", hash(FONT));
        assert_eq!(
            cache.read(&name).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(!root.0.join(&name).exists());
        assert_eq!(cache.read(&name).await.unwrap(), FONT);
        tokio::fs::write(root.0.join(&name), b"truncated")
            .await
            .unwrap();
        assert_eq!(cache.read(&name).await.unwrap(), FONT);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(tokio::fs::read(root.0.join(&name)).await.unwrap(), FONT);
        server.abort();
    }

    #[test]
    fn only_content_addressed_font_paths_can_reach_the_cdn() {
        let name = format!("{}.woff2", hash(FONT));
        assert_eq!(filename(&format!("fonts/{name}")), Some(name.as_str()));
        for path in [
            "fonts/../../secret",
            "fonts/https://example.com/file.woff2",
            "fonts/missing.woff2",
            "fonts/index.css",
            "fonts/../index.html",
        ] {
            assert_eq!(filename(path), None, "{path}");
        }
    }
}
