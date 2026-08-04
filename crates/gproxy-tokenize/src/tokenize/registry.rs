//! Global HF-tokenizer registry (§6.3): bundled deepseek vocab, persisted
//! vocabs through the [`PersistenceBackend`] (the native database backend uses
//! BLOB rows), and a fire-and-forget
//! background hydrate/HF-download path through the shared [`UpstreamClient`].
//! Native-only (`count-local` feature); tiktoken builtins are handled
//! directly by [`super::count`] and never live here.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use dashmap::DashMap;
use tokenizers::Tokenizer;

#[async_trait::async_trait]
pub trait TokenizerStore: Send + Sync {
    async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>>;
    async fn get_tokenizer_vocab(&self, name: &str) -> anyhow::Result<Option<Vec<u8>>>;
    async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()>;

    /// Isolate a persisted tokenizer that cannot be safely parsed. Backends
    /// may override this to move/delete the bad row; the default is a no-op.
    async fn quarantine_tokenizer_vocab(&self, _name: &str, _reason: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait TokenizerClient: Send + Sync {
    async fn send(&self, req: http::Request<Bytes>) -> anyhow::Result<http::Response<Bytes>>;
}

/// Bundled DeepSeek vocab, vendored from `deepseek-ai/DeepSeek-V4-Pro`
/// (`tokenizer.json`).
#[cfg(feature = "bundled-fallback")]
static DEEPSEEK: &[u8] = include_bytes!("../../assets/tokenizers/deepseek-v4-pro.tokenizer.json");
/// Names the bundled vocab answers to.
#[cfg(feature = "bundled-fallback")]
const BUNDLED_NAMES: &[&str] = &["deepseek", "deepseek-v4-pro"];

/// Bundled vocab, parsed AT MOST ONCE per process. Parsing the 6.3MB JSON
/// costs ~100ms; the `OnceLock` both caches the result and dedupes concurrent
/// first accesses (losers wait on the same init instead of re-parsing).
/// `None` is sticky on a parse failure — the asset is compile-time fixed, so
/// retrying cannot succeed.
#[cfg(feature = "bundled-fallback")]
static BUNDLED: std::sync::OnceLock<Option<Arc<Tokenizer>>> = std::sync::OnceLock::new();

#[cfg(feature = "bundled-fallback")]
fn bundled_tokenizer() -> Option<Arc<Tokenizer>> {
    BUNDLED
        .get_or_init(|| match Tokenizer::from_bytes(DEEPSEEK) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                tracing::error!(error = %e, "bundled tokenizer failed to parse");
                None
            }
        })
        .clone()
}

/// Where a vocab comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VocabSource {
    BuiltinTiktoken,
    Bundled,
    Downloaded,
}

/// Listing entry for the admin surface.
#[derive(Debug, Clone)]
pub struct VocabInfo {
    pub name: String,
    pub source: VocabSource,
    pub loaded: bool,
}

type LoadedMap = Arc<DashMap<String, Arc<Tokenizer>>>;

pub const MAX_TOKENIZER_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadRequestStatus {
    Scheduled,
    AlreadyInFlight,
    NegativeCached,
    NoRuntime,
}

/// Global tokenizer registry living on `AppState`.
pub struct TokenizerRegistry {
    /// Persisted vocab tier (BLOBs in the native database backend).
    store: Arc<dyn TokenizerStore>,
    /// Mirrors `instance_settings.enable_tokenizer_download`.
    download_enabled: AtomicBool,
    upstream: Arc<dyn TokenizerClient>,
    loaded: LoadedMap,
    inflight: Arc<DashMap<String, ()>>,
    negative: Arc<DashMap<String, ()>>,
}

impl TokenizerRegistry {
    pub fn new(store: Arc<dyn TokenizerStore>, upstream: Arc<dyn TokenizerClient>) -> Self {
        Self {
            store,
            download_enabled: AtomicBool::new(false),
            upstream,
            loaded: Arc::new(DashMap::new()),
            inflight: Arc::new(DashMap::new()),
            negative: Arc::new(DashMap::new()),
        }
    }

    pub fn set_download_enabled(&self, on: bool) {
        self.download_enabled.store(on, Ordering::Relaxed);
        if on {
            self.negative.clear();
        }
    }

    /// Builtins + bundled + persisted vocabs (admin surface; async because it
    /// asks the persistence backend).
    pub async fn list(&self) -> Vec<VocabInfo> {
        let mut out = vec![
            info("o200k_base", VocabSource::BuiltinTiktoken, true),
            info("cl100k_base", VocabSource::BuiltinTiktoken, true),
        ];
        #[cfg(feature = "bundled-fallback")]
        out.push(info(
            BUNDLED_NAMES[0],
            VocabSource::Bundled,
            self.loaded.contains_key(BUNDLED_NAMES[0]),
        ));
        match self.store.list_tokenizer_vocabs().await {
            Ok(names) => {
                for name in names {
                    let loaded = self.loaded.contains_key(&name);
                    out.push(info(&name, VocabSource::Downloaded, loaded));
                }
            }
            Err(e) => tracing::warn!(error = %e, "listing persisted tokenizer vocabs failed"),
        }
        out
    }

    /// memory → bundled name → `None`. Persisted/downloaded vocabs only show
    /// up after a background [`request_load`](Self::request_load) hydrates
    /// them into memory; a miss here never blocks the request.
    pub fn resolve(&self, name: &str) -> Option<Arc<Tokenizer>> {
        if let Some(t) = self.loaded.get(name) {
            return Some(Arc::clone(&t));
        }
        #[cfg(feature = "bundled-fallback")]
        if BUNDLED_NAMES.contains(&name) {
            let tok = bundled_tokenizer()?;
            for n in BUNDLED_NAMES {
                self.loaded.insert((*n).to_owned(), Arc::clone(&tok));
            }
            return Some(tok);
        }
        None
    }

    /// Fire-and-forget warm-up of the bundled vocab on the blocking pool, so
    /// the first count request never pays the parse inline. Call once at boot.
    pub fn preheat(&self) -> LoadRequestStatus {
        #[cfg(not(feature = "bundled-fallback"))]
        return LoadRequestStatus::NegativeCached;
        #[cfg(feature = "bundled-fallback")]
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return LoadRequestStatus::NoRuntime;
        };
        #[cfg(feature = "bundled-fallback")]
        let loaded = Arc::clone(&self.loaded);
        #[cfg(feature = "bundled-fallback")]
        runtime.spawn_blocking(move || {
            if let Some(tok) = bundled_tokenizer() {
                for n in BUNDLED_NAMES {
                    loaded.insert((*n).to_owned(), Arc::clone(&tok));
                }
            }
        });
        #[cfg(feature = "bundled-fallback")]
        return LoadRequestStatus::Scheduled;
    }

    /// Fire-and-forget load pipeline, deduped per name: hydrate from the
    /// persistence backend; when absent there, downloads are enabled, and the
    /// name is an HF repo path (`org/repo`), download
    /// `hf.co/{name}/resolve/main/tokenizer.json` through the shared upstream
    /// client and persist it. Never blocks the calling request.
    pub fn request_load(&self, name: &str) -> LoadRequestStatus {
        if self.negative.contains_key(name) {
            return LoadRequestStatus::NegativeCached;
        }
        if self.inflight.insert(name.to_owned(), ()).is_some() {
            return LoadRequestStatus::AlreadyInFlight;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.inflight.remove(name);
            return LoadRequestStatus::NoRuntime;
        };
        let store = Arc::clone(&self.store);
        let upstream = Arc::clone(&self.upstream);
        let loaded = Arc::clone(&self.loaded);
        let inflight = Arc::clone(&self.inflight);
        let negative = Arc::clone(&self.negative);
        let download_enabled = self.download_enabled.load(Ordering::Relaxed);
        let name = name.to_owned();
        runtime.spawn(async move {
            match load(store, upstream, &name, &loaded, download_enabled).await {
                Ok(LoadOutcome::Loaded) => {
                    negative.remove(&name);
                }
                Ok(LoadOutcome::Missing) => {
                    negative.insert(name.clone(), ());
                }
                Err(e) => {
                    negative.insert(name.clone(), ());
                    tracing::warn!(name, error = %e, "tokenizer load failed");
                }
            }
            inflight.remove(&name);
        });
        LoadRequestStatus::Scheduled
    }

    /// Resolve immediately or wait for persistence/download hydration.
    pub async fn resolve_or_load(&self, name: &str) -> anyhow::Result<Option<Arc<Tokenizer>>> {
        if let Some(tokenizer) = self.resolve(name) {
            return Ok(Some(tokenizer));
        }
        if self.negative.contains_key(name) {
            return Ok(None);
        }
        match load(
            Arc::clone(&self.store),
            Arc::clone(&self.upstream),
            name,
            &self.loaded,
            self.download_enabled.load(Ordering::Relaxed),
        )
        .await?
        {
            LoadOutcome::Loaded => {
                self.negative.remove(name);
                Ok(self.resolve(name))
            }
            LoadOutcome::Missing => {
                self.negative.insert(name.to_owned(), ());
                Ok(None)
            }
        }
    }
}

enum LoadOutcome {
    Loaded,
    Missing,
}

/// Hydrate `name` from the store, falling back to an HF download.
async fn load(
    store: Arc<dyn TokenizerStore>,
    upstream: Arc<dyn TokenizerClient>,
    name: &str,
    loaded: &LoadedMap,
    download_enabled: bool,
) -> anyhow::Result<LoadOutcome> {
    if let Some(bytes) = store.get_tokenizer_vocab(name).await? {
        let parsed = if bytes.len() > MAX_TOKENIZER_BYTES {
            Err(anyhow::anyhow!(
                "persisted vocab exceeds {} bytes",
                MAX_TOKENIZER_BYTES
            ))
        } else {
            Tokenizer::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("bad persisted vocab: {e}"))
        };
        match parsed {
            Ok(tokenizer) => {
                loaded.insert(name.to_owned(), Arc::new(tokenizer));
                return Ok(LoadOutcome::Loaded);
            }
            Err(error) => {
                store
                    .quarantine_tokenizer_vocab(name, &error.to_string())
                    .await?;
                tracing::warn!(name, error = %error, "persisted tokenizer quarantined");
                if !download_enabled {
                    return Err(error);
                }
            }
        }
    }
    if !download_enabled {
        return Ok(LoadOutcome::Missing);
    }

    validate_hf_repo_id(name)?;

    let url = format!("https://huggingface.co/{name}/resolve/main/tokenizer.json");
    let req = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .body(Bytes::new())?;
    let resp = upstream.send(req).await?;
    anyhow::ensure!(resp.status().is_success(), "HTTP {}", resp.status());
    let body = resp.into_body();
    anyhow::ensure!(
        body.len() <= MAX_TOKENIZER_BYTES,
        "downloaded vocab exceeds {} bytes",
        MAX_TOKENIZER_BYTES
    );
    let tok = Tokenizer::from_bytes(&body).map_err(|e| anyhow::anyhow!("bad vocab: {e}"))?;

    store.put_tokenizer_vocab(name, &body).await?;
    loaded.insert(name.to_owned(), Arc::new(tok));
    tracing::info!(name, "tokenizer downloaded");
    Ok(LoadOutcome::Loaded)
}

fn validate_hf_repo_id(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(name.len() <= 200, "HF repo id is too long");
    let parts: Vec<_> = name.split('/').collect();
    anyhow::ensure!(parts.len() == 2, "HF repo id must be `owner/repository`");
    for part in parts {
        anyhow::ensure!(
            !part.is_empty() && part.len() <= 96,
            "invalid HF repo segment"
        );
        anyhow::ensure!(
            part.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
            "invalid character in HF repo id"
        );
        anyhow::ensure!(
            !part.starts_with('.')
                && !part.starts_with('-')
                && !part.ends_with('.')
                && !part.ends_with('-'),
            "invalid HF repo segment boundary"
        );
        anyhow::ensure!(!part.contains(".."), "invalid HF repo traversal sequence");
    }
    Ok(())
}

fn info(name: &str, source: VocabSource, loaded: bool) -> VocabInfo {
    VocabInfo {
        name: name.to_owned(),
        source,
        loaded,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct CountingStore(AtomicUsize);

    #[async_trait::async_trait]
    impl TokenizerStore for CountingStore {
        async fn list_tokenizer_vocabs(&self) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn get_tokenizer_vocab(&self, _: &str) -> anyhow::Result<Option<Vec<u8>>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(None)
        }

        async fn put_tokenizer_vocab(&self, _: &str, _: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoClient;

    #[async_trait::async_trait]
    impl TokenizerClient for NoClient {
        async fn send(
            &self,
            _: http::Request<Bytes>,
        ) -> anyhow::Result<http::Response<Bytes>> {
            anyhow::bail!("network should not be used")
        }
    }

    #[tokio::test]
    async fn negative_cache_avoids_repeated_store_misses() {
        let store = Arc::new(CountingStore(AtomicUsize::new(0)));
        let registry = TokenizerRegistry::new(store.clone(), Arc::new(NoClient));
        assert!(registry.resolve_or_load("unknown").await.unwrap().is_none());
        assert!(registry.resolve_or_load("unknown").await.unwrap().is_none());
        assert_eq!(store.0.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn validates_hugging_face_repo_ids() {
        assert!(validate_hf_repo_id("owner/model-name").is_ok());
        assert!(validate_hf_repo_id("owner/model/extra").is_err());
        assert!(validate_hf_repo_id("../model").is_err());
        assert!(validate_hf_repo_id("owner/model?revision=main").is_err());
    }
}
