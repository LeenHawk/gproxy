//! Content-addressed S3/R2/MinIO resolver using gproxy-file's OpenDAL Operator.
//! Explicit credentials only; no local-file/IMDS discovery or media URL fetching.
use crate::*;
use gproxy_file::{Operator, S3};
use sha2::{Digest, Sha256};
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    feature = "embeddings"
))]
use std::future::Future;
use std::time::Duration;

/// Intentionally not Debug/Serialize: credentials do not belong in IR or logs.
pub struct S3AssetConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub key_prefix: String,
    pub presign_expiry: Duration,
    /// False uses path-style addressing (also suitable for MinIO and R2).
    pub virtual_host_style: bool,
}
impl S3AssetConfig {
    pub fn new(
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            region: region.into(),
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            key_prefix: String::new(),
            presign_expiry: Duration::from_secs(300),
            virtual_host_style: false,
        }
    }
    /// OpenDAL handles endpoint/addressing rules and validates S3 configuration.
    /// This builder can also be passed to gproxy_file::s3_with_transport.
    pub fn builder(&self) -> S3 {
        let mut builder = S3::default()
            .endpoint(&self.endpoint)
            .bucket(&self.bucket)
            .region(&self.region)
            .access_key_id(&self.access_key_id)
            .secret_access_key(&self.secret_access_key)
            .disable_config_load()
            .disable_ec2_metadata();
        if let Some(token) = &self.session_token {
            builder = builder.session_token(token);
        }
        if self.virtual_host_style {
            builder = builder.enable_virtual_host_style();
        }
        builder
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AssetData {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// Keep this store alongside the client; signed URLs are never cached here.
pub struct S3AssetResolver {
    operator: Operator,
    key_prefix: String,
    presign_expiry: Duration,
}
impl S3AssetResolver {
    pub fn new(config: S3AssetConfig) -> Result<Self> {
        let operator = gproxy_file::s3(config.builder()).map_err(storage_error)?;
        Ok(Self::from_operator(
            operator,
            config.key_prefix,
            config.presign_expiry,
        ))
    }
    /// Inject a configured S3 operator, for example with a host HTTP transport.
    /// It must support a presigned GET usable without custom request headers.
    pub fn from_operator(
        operator: Operator,
        key_prefix: impl Into<String>,
        presign_expiry: Duration,
    ) -> Self {
        let prefix = key_prefix.into();
        Self {
            operator,
            key_prefix: prefix.trim_matches('/').to_owned(),
            presign_expiry,
        }
    }
    /// SHA-256 of content, with a canonical extension. Application references
    /// never become object paths. Unknown MIME types use a .bin extension.
    pub fn object_key(&self, data: &AssetData) -> String {
        let hash: String = Sha256::digest(&data.bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let extension = extension(&data.media_type);
        if self.key_prefix.is_empty() {
            format!("{hash}.{extension}")
        } else {
            format!("{}/{hash}.{extension}", self.key_prefix)
        }
    }
    /// Adapter for the generic round-3 resolver. Callers supply only the bytes;
    /// format/size validation happens before any storage operation.
    pub async fn resolve(&self, input: ResolveAsset, data: AssetData) -> Result<ResolvedAsset> {
        if input
            .media_type
            .as_deref()
            .is_some_and(|mime| mime != data.media_type)
        {
            return Err(Error::invalid(
                "resolver MIME type differs from the declared media type",
            ));
        }
        let capability = input.capability.for_mime(Some(&data.media_type));
        // Follow the caller's mode as narrowed by the MIME type; the store is only
        // touched when the result is a URL.
        let (method, constraints) = match (input.preferred, capability.url, capability.inline) {
            (DeliveryMethod::Url, Some(url), _) | (DeliveryMethod::Inline, Some(url), None) => {
                (DeliveryMethod::Url, url)
            }
            (_, _, Some(inline)) => (DeliveryMethod::Inline, inline),
            (_, None, None) => {
                return Err(Error::invalid(
                    "no supported asset delivery method for the resolved MIME type",
                ));
            }
        };
        let size = data.bytes.len() as u64;
        constraints.check(Some(&data.media_type), Some(size))?;
        match method {
            DeliveryMethod::Inline => Ok(ResolvedAsset::Bytes {
                bytes: data.bytes,
                media_type: data.media_type,
            }),
            DeliveryMethod::Url => {
                let key = self.object_key(&data);
                match self.operator.stat(&key).await {
                    Ok(_) => {}
                    Err(error) if error.kind() == gproxy_file::ErrorKind::NotFound => {
                        self.operator
                            .write_with(&key, data.bytes)
                            .content_type(&data.media_type)
                            .await
                            .map_err(storage_error)?;
                    }
                    Err(error) => return Err(storage_error(error)),
                }
                let signed = self
                    .operator
                    .presign_read(&key, self.presign_expiry)
                    .await
                    .map_err(storage_error)?;
                if signed.header().keys().any(|key| key != http::header::HOST) {
                    return Err(Error::invalid(
                        "presigned asset GET requires custom headers that a vendor URL fetch cannot supply",
                    ));
                }
                Ok(ResolvedAsset::Url {
                    url: signed.uri().to_string(),
                    media_type: Some(data.media_type),
                    byte_len: Some(size),
                })
            }
        }
    }
}
fn extension(mime: &str) -> &'static str {
    match mime.split(';').next().unwrap_or(mime) {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/avif" => "avif",
        "image/heic" => "heic",
        "image/heif" => "heif",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "text/csv" => "csv",
        "application/json" => "json",
        "audio/wav" | "audio/x-wav" | "video/audio/wav" => "wav",
        "audio/mp3" | "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "audio/m4a" => "m4a",
        "audio/pcm" | "audio/l16" | "audio/L16" | "video/audio/s16le" => "pcm",
        "video/mp4" => "mp4",
        "video/mpeg" | "video/mpg" => "mpeg",
        "video/webm" => "webm",
        "video/mov" => "mov",
        "video/avi" => "avi",
        "video/x-flv" => "flv",
        "video/wmv" => "wmv",
        "video/3gpp" => "3gp",
        _ => "bin",
    }
}
fn storage_error(error: gproxy_file::Error) -> Error {
    use gproxy_file::ErrorKind as K;
    let kind = match error.kind() {
        K::PermissionDenied => ErrorKind::Auth,
        K::NotFound => ErrorKind::NotFound,
        K::RateLimited => ErrorKind::RateLimit,
        K::ConfigInvalid | K::Unsupported => ErrorKind::InvalidData,
        _ => ErrorKind::Upstream,
    };
    Error {
        kind,
        status: None,
        retryable: error.is_temporary(),
        body: None,
        retry_after: None,
        message: error.to_string(),
    }
}

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl GenerateRequest {
    pub async fn resolve_assets_s3<F, Fut>(
        &self,
        store: &S3AssetResolver,
        mut load: F,
    ) -> Result<PreparedGenerateRequest>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<AssetData>>,
    {
        self.resolve_assets(DeliveryMode::PreferUrl, |input| {
            let data = load(input.reference.clone());
            async move { store.resolve(input, data.await?).await }
        })
        .await
    }
}
#[cfg(feature = "embeddings")]
impl crate::embeddings::EmbedRequest {
    pub async fn resolve_assets_s3<F, Fut>(
        &self,
        store: &S3AssetResolver,
        mut load: F,
    ) -> Result<crate::embeddings::PreparedEmbedRequest>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<AssetData>>,
    {
        self.resolve_assets(DeliveryMode::PreferUrl, |input| {
            let data = load(input.reference.clone());
            async move { store.resolve(input, data.await?).await }
        })
        .await
    }
}

/// A client configured with S3 asset delivery. The inner client remains available
/// for native operations that do not use the conversation asset resolver.
pub struct S3Client<B> {
    pub client: Client<B>,
    pub assets: S3AssetResolver,
}
impl<B> Client<B> {
    pub fn with_s3(self, assets: S3AssetResolver) -> S3Client<B> {
        S3Client {
            client: self,
            assets,
        }
    }
}
impl<B: Backend> S3Client<B> {
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn generate<F, Fut>(
        &self,
        request: &GenerateRequest,
        load: F,
    ) -> Result<GenerateResult>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<AssetData>>,
    {
        let prepared = request.resolve_assets_s3(&self.assets, load).await?;
        self.client.generate_prepared(prepared).await
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn stream<F, Fut>(
        &self,
        request: &GenerateRequest,
        load: F,
    ) -> Result<GenerationStream>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<AssetData>>,
    {
        let prepared = request.resolve_assets_s3(&self.assets, load).await?;
        self.client.stream_prepared(prepared).await
    }
    #[cfg(feature = "embeddings")]
    pub async fn embed<F, Fut>(
        &self,
        request: &crate::embeddings::EmbedRequest,
        load: F,
    ) -> Result<crate::embeddings::EmbedResult>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<AssetData>>,
    {
        let prepared = request.resolve_assets_s3(&self.assets, load).await?;
        self.client.embed_prepared(prepared).await
    }
}

#[cfg(any(feature = "images", feature = "speech", feature = "transcription"))]
mod media;
