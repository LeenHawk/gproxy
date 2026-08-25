use std::sync::Arc;

use dashmap::DashMap;
use tokenizers::Tokenizer;

use super::{LoadRequestStatus, RegistryError, TokenizerClient, TokenizerRegistry, TokenizerStore};

pub const MAX_TOKENIZER_BYTES: usize = 16 * 1024 * 1024;
type Loaded = Arc<DashMap<String, Arc<Tokenizer>>>;

impl TokenizerRegistry {
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
        let download_enabled = self
            .download_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        let name = name.to_owned();
        runtime.spawn(async move {
            match load(store, upstream, &name, &loaded, download_enabled).await {
                Ok(LoadOutcome::Loaded) => {
                    negative.remove(&name);
                }
                Ok(LoadOutcome::Missing) | Err(_) => {
                    negative.insert(name.clone(), ());
                }
            }
            inflight.remove(&name);
        });
        LoadRequestStatus::Scheduled
    }

    pub async fn resolve_or_load(
        &self,
        name: &str,
    ) -> Result<Option<Arc<Tokenizer>>, RegistryError> {
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
            self.download_enabled
                .load(std::sync::atomic::Ordering::Relaxed),
        )
        .await?
        {
            LoadOutcome::Loaded => Ok(self.resolve(name)),
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

async fn load(
    store: Arc<dyn TokenizerStore>,
    upstream: Arc<dyn TokenizerClient>,
    name: &str,
    loaded: &Loaded,
    download_enabled: bool,
) -> Result<LoadOutcome, RegistryError> {
    if let Some(bytes) = store.get(name).await? {
        match parse(bytes).await {
            Ok(tokenizer) => {
                loaded.insert(name.to_owned(), Arc::new(tokenizer));
                return Ok(LoadOutcome::Loaded);
            }
            Err(error) => {
                store.quarantine(name, &error.to_string()).await?;
                if !download_enabled {
                    return Err(error);
                }
            }
        }
    }
    if !download_enabled {
        return Ok(LoadOutcome::Missing);
    }
    validate_repo_id(name)?;
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!(
            "https://huggingface.co/{name}/resolve/main/tokenizer.json"
        ))
        .body(bytes::Bytes::new())
        .map_err(|error| RegistryError::new(error.to_string()))?;
    let response = upstream.send(request).await?;
    if !response.status().is_success() {
        return Err(RegistryError::new(format!("HTTP {}", response.status())));
    }
    let bytes = response.into_body();
    let tokenizer = parse(bytes.to_vec()).await?;
    store.put(name, &bytes).await?;
    loaded.insert(name.to_owned(), Arc::new(tokenizer));
    Ok(LoadOutcome::Loaded)
}

async fn parse(bytes: Vec<u8>) -> Result<Tokenizer, RegistryError> {
    if bytes.len() > MAX_TOKENIZER_BYTES {
        return Err(RegistryError::new("tokenizer exceeds 16 MiB"));
    }
    tokio::task::spawn_blocking(move || {
        Tokenizer::from_bytes(bytes).map_err(|error| RegistryError::new(error.to_string()))
    })
    .await
    .map_err(|error| RegistryError::new(error.to_string()))?
}

fn validate_repo_id(name: &str) -> Result<(), RegistryError> {
    let parts: Vec<_> = name.split('/').collect();
    let valid = name.len() <= 200
        && parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 96
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
                && !part.starts_with(['.', '-'])
                && !part.ends_with(['.', '-'])
                && !part.contains("..")
        });
    valid
        .then_some(())
        .ok_or_else(|| RegistryError::new("invalid Hugging Face repository id"))
}
