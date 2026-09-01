use std::sync::Arc;

use dashmap::DashMap;
use tokenizers::Tokenizer;

use super::{
    LoadRequestStatus, ProgressReporter, RegistryError, TokenizerClient, TokenizerDownloadProgress,
    TokenizerRegistry, TokenizerStore,
};

pub const MAX_TOKENIZER_BYTES: usize = 32 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
type Loaded = Arc<DashMap<String, Arc<Tokenizer>>>;
type Downloads = Arc<DashMap<String, TokenizerDownloadProgress>>;
type HuggingFaceToken = Arc<std::sync::RwLock<Option<String>>>;

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
        let hugging_face_token = Arc::clone(&self.hugging_face_token);
        let inflight = Arc::clone(&self.inflight);
        let negative = Arc::clone(&self.negative);
        let downloads = Arc::clone(&self.downloads);
        let download_enabled = self
            .download_enabled
            .load(std::sync::atomic::Ordering::Relaxed);
        let name = name.to_owned();
        runtime.spawn(async move {
            match load(
                store,
                upstream,
                &name,
                &loaded,
                &hugging_face_token,
                &downloads,
                download_enabled,
            )
            .await
            {
                Ok(LoadOutcome::Loaded) => {
                    negative.remove(&name);
                }
                Ok(LoadOutcome::Missing) => {
                    negative.insert(name.clone(), ());
                }
                Err(error) => {
                    negative.insert(name.clone(), ());
                    tracing::warn!(name, %error, "tokenizer load failed");
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
            &self.hugging_face_token,
            &self.downloads,
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

    pub async fn fetch(
        &self,
        name: &str,
        repository: &str,
    ) -> Result<Arc<Tokenizer>, RegistryError> {
        validate_vocab_name(name)?;
        validate_repo_id(repository)?;
        if let Some(stored) = self.store.get(name).await?
            && stored.repository == repository
        {
            match parse(stored.bytes).await {
                Ok(tokenizer) => {
                    let tokenizer = Arc::new(tokenizer);
                    self.loaded.insert(name.to_owned(), Arc::clone(&tokenizer));
                    self.negative.remove(name);
                    return Ok(tokenizer);
                }
                Err(error) => self.store.quarantine(name, &error.to_string()).await?,
            }
        }
        download(
            Arc::clone(&self.store),
            Arc::clone(&self.upstream),
            name,
            repository,
            &self.loaded,
            &self.hugging_face_token,
            &self.downloads,
        )
        .await?;
        self.negative.remove(name);
        self.resolve(name)
            .ok_or_else(|| RegistryError::new("tokenizer was not loaded"))
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
    hugging_face_token: &HuggingFaceToken,
    downloads: &Downloads,
    download_enabled: bool,
) -> Result<LoadOutcome, RegistryError> {
    validate_vocab_name(name)?;
    let mut repository = name.to_owned();
    if let Some(stored) = store.get(name).await? {
        repository = stored.repository;
        match parse(stored.bytes).await {
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
    validate_repo_id(&repository)?;
    download(
        store,
        upstream,
        name,
        &repository,
        loaded,
        hugging_face_token,
        downloads,
    )
    .await?;
    Ok(LoadOutcome::Loaded)
}

async fn download(
    store: Arc<dyn TokenizerStore>,
    upstream: Arc<dyn TokenizerClient>,
    name: &str,
    repository: &str,
    loaded: &Loaded,
    hugging_face_token: &HuggingFaceToken,
    downloads: &Downloads,
) -> Result<(), RegistryError> {
    let mut builder = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!(
            "https://huggingface.co/{repository}/resolve/main/tokenizer.json"
        ))
        .header(http::header::ACCEPT_ENCODING, "identity");
    if let Some(token) = hugging_face_token
        .read()
        .ok()
        .and_then(|token| token.clone())
    {
        let authorization = http::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| RegistryError::new("invalid Hugging Face token"))?;
        builder = builder.header(http::header::AUTHORIZATION, authorization);
    }
    let request = builder
        .body(bytes::Bytes::new())
        .map_err(|error| RegistryError::new(error.to_string()))?;
    downloads.insert(name.to_owned(), TokenizerDownloadProgress::default());
    let progress_name = name.to_owned();
    let progress_downloads = Arc::clone(downloads);
    let reporter: ProgressReporter = Arc::new(move |progress| {
        progress_downloads.insert(progress_name.clone(), progress);
    });
    let result = async {
        let response = send_following_redirects(&upstream, request, reporter).await?;
        if !response.status().is_success() {
            return Err(RegistryError::new(format!("HTTP {}", response.status())));
        }
        let bytes = response.into_body();
        let tokenizer = parse(bytes.to_vec()).await?;
        store.put(name, repository, &bytes).await?;
        loaded.insert(name.to_owned(), Arc::new(tokenizer));
        Ok(())
    }
    .await;
    downloads.remove(name);
    result
}

async fn send_following_redirects(
    upstream: &Arc<dyn TokenizerClient>,
    mut request: http::Request<bytes::Bytes>,
    reporter: ProgressReporter,
) -> Result<http::Response<bytes::Bytes>, RegistryError> {
    for redirects in 0..=MAX_REDIRECTS {
        let current = request.uri().clone();
        let authorization = request.headers().get(http::header::AUTHORIZATION).cloned();
        let response = upstream.send(request, Some(Arc::clone(&reporter))).await?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        if redirects == MAX_REDIRECTS {
            return Err(RegistryError::new("too many tokenizer redirects"));
        }
        let location = response
            .headers()
            .get(http::header::LOCATION)
            .ok_or_else(|| RegistryError::new("tokenizer redirect is missing Location"))?;
        let uri = redirect_uri(&current, location)?;
        let keep_authorization = uri.host() == Some("huggingface.co");
        let mut builder = http::Request::builder()
            .method(http::Method::GET)
            .uri(uri)
            .header(http::header::ACCEPT_ENCODING, "identity");
        if let Some(authorization) = authorization
            && keep_authorization
        {
            builder = builder.header(http::header::AUTHORIZATION, authorization);
        }
        request = builder
            .body(bytes::Bytes::new())
            .map_err(|error| RegistryError::new(error.to_string()))?;
    }
    Err(RegistryError::new("too many tokenizer redirects"))
}

fn redirect_uri(
    current: &http::Uri,
    location: &http::HeaderValue,
) -> Result<http::Uri, RegistryError> {
    let location = location
        .to_str()
        .map_err(|_| RegistryError::new("invalid tokenizer redirect"))?;
    let uri = if location.starts_with('/') {
        format!(
            "{}://{}{}",
            current.scheme_str().unwrap_or("https"),
            current
                .authority()
                .ok_or_else(|| RegistryError::new("tokenizer redirect has no authority"))?,
            location
        )
        .parse::<http::Uri>()
    } else {
        location.parse::<http::Uri>()
    }
    .map_err(|_| RegistryError::new("invalid tokenizer redirect"))?;
    let host = uri
        .host()
        .ok_or_else(|| RegistryError::new("tokenizer redirect has no host"))?;
    let trusted_host =
        host == "huggingface.co" || host.ends_with(".huggingface.co") || host.ends_with(".hf.co");
    if uri.scheme_str() != Some("https") || !trusted_host {
        return Err(RegistryError::new("untrusted tokenizer redirect"));
    }
    Ok(uri)
}

async fn parse(bytes: Vec<u8>) -> Result<Tokenizer, RegistryError> {
    if bytes.len() > MAX_TOKENIZER_BYTES {
        return Err(RegistryError::new("tokenizer exceeds 32 MiB"));
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

fn validate_vocab_name(name: &str) -> Result<(), RegistryError> {
    let valid = !name.is_empty()
        && name.len() <= 200
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
        && !name.starts_with(['.', '-', '/'])
        && !name.ends_with(['.', '-', '/'])
        && !name.contains("..")
        && !name.contains("//");
    valid
        .then_some(())
        .ok_or_else(|| RegistryError::new("invalid tokenizer vocabulary name"))
}
