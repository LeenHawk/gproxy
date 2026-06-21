//! Global HF-tokenizer registry (§6.3): bundled deepseek vocab, persisted
//! vocabs through the [`PersistenceBackend`] (file backend = raw files under
//! `data_dir/tokenizers/`, db backend = BLOB rows), and a fire-and-forget
//! background hydrate/HF-download path through the shared [`UpstreamClient`].
//! Native-only (`count-local` feature); tiktoken builtins are handled
//! directly by [`super::count`] and never live here.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use dashmap::DashMap;
use tokenizers::Tokenizer;

use crate::http::client::UpstreamClient;
use crate::store::persistence::PersistenceBackend;

/// Bundled DeepSeek vocab, vendored from `deepseek-ai/DeepSeek-V4-Pro`
/// (`tokenizer.json`).
static DEEPSEEK: &[u8] = include_bytes!("../../assets/tokenizers/deepseek-v4-pro.tokenizer.json");
/// Names the bundled vocab answers to.
const BUNDLED_NAMES: &[&str] = &["deepseek", "deepseek-v4-pro"];

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

/// Global tokenizer registry living on `AppState`.
pub struct TokenizerRegistry {
    /// Persisted vocab tier (file backend = raw files, db backend = BLOBs).
    store: Arc<dyn PersistenceBackend>,
    /// Mirrors `instance_settings.enable_tokenizer_download`.
    download_enabled: AtomicBool,
    upstream: Arc<dyn UpstreamClient>,
    loaded: LoadedMap,
    inflight: Arc<DashMap<String, ()>>,
}

impl TokenizerRegistry {
    pub fn new(store: Arc<dyn PersistenceBackend>, upstream: Arc<dyn UpstreamClient>) -> Self {
        Self {
            store,
            download_enabled: AtomicBool::new(false),
            upstream,
            loaded: Arc::new(DashMap::new()),
            inflight: Arc::new(DashMap::new()),
        }
    }

    pub fn set_download_enabled(&self, on: bool) {
        self.download_enabled.store(on, Ordering::Relaxed);
    }

    /// Builtins + bundled + persisted vocabs (admin surface; async because it
    /// asks the persistence backend).
    pub async fn list(&self) -> Vec<VocabInfo> {
        let mut out = vec![
            info("o200k_base", VocabSource::BuiltinTiktoken, true),
            info("cl100k_base", VocabSource::BuiltinTiktoken, true),
            info(
                BUNDLED_NAMES[0],
                VocabSource::Bundled,
                self.loaded.contains_key(BUNDLED_NAMES[0]),
            ),
        ];
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
        if BUNDLED_NAMES.contains(&name) {
            let tok = Arc::new(Tokenizer::from_bytes(DEEPSEEK).ok()?);
            for n in BUNDLED_NAMES {
                self.loaded.insert((*n).to_owned(), Arc::clone(&tok));
            }
            return Some(tok);
        }
        None
    }

    /// Fire-and-forget load pipeline, deduped per name: hydrate from the
    /// persistence backend; when absent there, downloads are enabled, and the
    /// name is an HF repo path (`org/repo`), download
    /// `hf.co/{name}/resolve/main/tokenizer.json` through the shared upstream
    /// client and persist it. Never blocks the calling request.
    pub fn request_load(&self, name: &str) {
        if self.inflight.insert(name.to_owned(), ()).is_some() {
            return;
        }
        let store = Arc::clone(&self.store);
        let upstream = Arc::clone(&self.upstream);
        let loaded = Arc::clone(&self.loaded);
        let inflight = Arc::clone(&self.inflight);
        let download_enabled = self.download_enabled.load(Ordering::Relaxed);
        let name = name.to_owned();
        tokio::spawn(async move {
            if let Err(e) = load(store, upstream, &name, &loaded, download_enabled).await {
                tracing::warn!(name, error = %e, "tokenizer load failed");
            }
            inflight.remove(&name);
        });
    }
}

/// Hydrate `name` from the store, falling back to an HF download.
async fn load(
    store: Arc<dyn PersistenceBackend>,
    upstream: Arc<dyn UpstreamClient>,
    name: &str,
    loaded: &LoadedMap,
    download_enabled: bool,
) -> anyhow::Result<()> {
    if let Some(bytes) = store.get_tokenizer_vocab(name).await? {
        let tok = Tokenizer::from_bytes(&bytes).map_err(|e| anyhow::anyhow!("bad vocab: {e}"))?;
        loaded.insert(name.to_owned(), Arc::new(tok));
        return Ok(());
    }
    if !download_enabled || !name.contains('/') {
        return Ok(());
    }

    let url = format!("https://huggingface.co/{name}/resolve/main/tokenizer.json");
    let req = http::Request::builder()
        .method(http::Method::GET)
        .uri(&url)
        .body(Bytes::new())?;
    let resp = upstream
        .send(req)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    anyhow::ensure!(resp.status().is_success(), "HTTP {}", resp.status());
    let body = resp.into_body();
    let tok = Tokenizer::from_bytes(&body).map_err(|e| anyhow::anyhow!("bad vocab: {e}"))?;

    store.put_tokenizer_vocab(name, &body).await?;
    loaded.insert(name.to_owned(), Arc::new(tok));
    tracing::info!(name, "tokenizer downloaded");
    Ok(())
}

fn info(name: &str, source: VocabSource, loaded: bool) -> VocabInfo {
    VocabInfo {
        name: name.to_owned(),
        source,
        loaded,
    }
}
