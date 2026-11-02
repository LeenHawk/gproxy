//! Asset preparation retains portable material and caches vendor-scoped upload
//! references. The caller owns application/blob I/O; this module never fetches URLs.
use crate::{protocol::Rest, *};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UploadedAsset {
    pub id: String,
    pub uri: Option<String>,
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub processing: bool,
    pub source_hash: String,
    pub media_type: String,
}
#[derive(Default, Serialize, Deserialize)]
struct UploadMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upload: Option<UploadedAsset>,
}

pub(crate) fn clock() -> Result<u64> {
    web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(Error::invalid)
}
pub fn uploaded(block: &ContentBlock<impl Sized>, vendor: Vendor) -> Result<Option<UploadedAsset>> {
    uploaded_from(&block.provider_metadata, vendor)
}
fn uploaded_from(metadata: &ProviderMetadata, vendor: Vendor) -> Result<Option<UploadedAsset>> {
    match metadata.get(&vendor) {
        None => Ok(None),
        Some(rest) => {
            serde_json::from_value::<UploadMetadata>(serde_json::Value::Object(rest.clone()))
                .map(|value| value.upload)
                .map_err(Error::invalid)
        }
    }
}
fn remember(metadata: &mut ProviderMetadata, vendor: Vendor, value: UploadedAsset) -> Result<()> {
    let encoded: Rest = serde_json::from_value(
        serde_json::to_value(UploadMetadata {
            upload: Some(value),
        })
        .map_err(Error::invalid)?,
    )
    .map_err(Error::invalid)?;
    metadata.entry(vendor).or_default().extend(encoded);
    Ok(())
}
pub(crate) fn source<'a>(
    block: &ContentBlock<impl Sized>,
    original: &'a Asset,
    vendor: Vendor,
    allow_files: bool,
) -> Result<Cow<'a, Asset>> {
    if let Asset::ProviderFile { vendor: owner, .. } = original
        && *owner != vendor
    {
        return Err(Error::invalid(
            "provider file IDs are vendor scoped; supply portable bytes or a URL before changing vendors",
        ));
    }
    if let Asset::ProviderFile { .. } = original {
        return Ok(Cow::Borrowed(original));
    }
    if allow_files
        && let Asset::Inline { bytes } = original
        && let Some(upload) = uploaded(block, vendor)?
    {
        if upload.source_hash != blake3::hash(bytes).to_hex().as_str() {
            return Err(Error::invalid(
                "asset changed; prepare_assets must refresh its upload",
            ));
        }
        let now = clock()?;
        if upload.expires_at.is_some_and(|expiry| expiry <= now) {
            return Err(Error::invalid(
                "uploaded asset expired; prepare_assets must refresh it",
            ));
        }
        if upload.processing {
            return Err(Error::invalid(
                "uploaded file is still processing; call prepare_assets again when ready",
            ));
        }
        return Ok(Cow::Owned(Asset::ProviderFile {
            vendor,
            id: upload.uri.unwrap_or(upload.id),
        }));
    }
    Ok(Cow::Borrowed(original))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedAsset {
    Bytes { bytes: Vec<u8>, media_type: String },
    Url { url: String },
}

/// A lower inline threshold is useful for Workers and for reusing Files IDs.
/// Hard vendor limits still apply; this threshold never permits a larger file.
#[derive(Debug, Clone, Copy, Default)]
pub struct PreparationOptions {
    pub inline_bytes: Option<u64>,
}
#[derive(Clone, Copy)]
enum Kind {
    Image,
    Document,
    File,
    Audio,
    Video,
}
#[derive(Clone, Copy)]
enum Context {
    Message,
    ToolResult,
}
fn vendor(dialect: Dialect) -> Vendor {
    dialect.vendor()
}

impl<B: Backend> Client<B> {
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn prepare_assets<F, Fut>(
        &self,
        request: &mut GenerateRequest,
        resolve: F,
    ) -> Result<()>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        self.prepare_assets_with(request, PreparationOptions::default(), resolve)
            .await
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn prepare_assets_with<F, Fut>(
        &self,
        request: &mut GenerateRequest,
        options: PreparationOptions,
        mut resolve: F,
    ) -> Result<()>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        let model = request.model.clone();
        let now = clock()?;
        for message in &mut request.messages {
            for block in &mut message.content {
                let metadata = &mut block.provider_metadata;
                match &mut block.content {
                    Content::Image(value) => {
                        self.prepare_one(
                            &model,
                            metadata,
                            &mut value.source,
                            &mut value.media_type,
                            None,
                            Kind::Image,
                            Context::Message,
                            options,
                            now,
                            &mut resolve,
                        )
                        .await?
                    }
                    Content::Document(value) => {
                        self.prepare_document(
                            &model,
                            metadata,
                            value,
                            Context::Message,
                            options,
                            now,
                            &mut resolve,
                        )
                        .await?
                    }
                    Content::File(value) => {
                        self.prepare_one(
                            &model,
                            metadata,
                            &mut value.source,
                            &mut value.media_type,
                            value.filename.as_deref(),
                            Kind::File,
                            Context::Message,
                            options,
                            now,
                            &mut resolve,
                        )
                        .await?
                    }
                    Content::Video(value) => {
                        self.prepare_one(
                            &model,
                            metadata,
                            &mut value.source,
                            &mut value.media_type,
                            None,
                            Kind::Video,
                            Context::Message,
                            options,
                            now,
                            &mut resolve,
                        )
                        .await?
                    }
                    Content::Audio(value) => {
                        #[cfg(feature = "chat")]
                        if model.dialect == Dialect::OpenaiChat && message.role == Role::Assistant {
                            continue;
                        }
                        let mut mime = Some(crate::dialects::media::pcm_mime(&value.format)?);
                        self.prepare_one(
                            &model,
                            metadata,
                            &mut value.source,
                            &mut mime,
                            None,
                            Kind::Audio,
                            Context::Message,
                            options,
                            now,
                            &mut resolve,
                        )
                        .await?;
                    }
                    Content::ToolResult(value) => {
                        if let ToolOutput::Blocks(blocks) = &mut value.output {
                            for block in blocks {
                                match &mut block.content {
                                    ToolResultContent::Text(_) => {}
                                    ToolResultContent::Image(value) => {
                                        self.prepare_one(
                                            &model,
                                            &mut block.provider_metadata,
                                            &mut value.source,
                                            &mut value.media_type,
                                            None,
                                            Kind::Image,
                                            Context::ToolResult,
                                            options,
                                            now,
                                            &mut resolve,
                                        )
                                        .await?
                                    }
                                    ToolResultContent::Document(value) => {
                                        self.prepare_document(
                                            &model,
                                            &mut block.provider_metadata,
                                            value,
                                            Context::ToolResult,
                                            options,
                                            now,
                                            &mut resolve,
                                        )
                                        .await?
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    #[allow(clippy::too_many_arguments)]
    async fn prepare_document<F, Fut>(
        &self,
        model: &Model,
        metadata: &mut ProviderMetadata,
        value: &mut Document,
        context: Context,
        options: PreparationOptions,
        now: u64,
        resolve: &mut F,
    ) -> Result<()>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        match &mut value.source {
            DocumentSource::Asset { source } => {
                self.prepare_one(
                    model,
                    metadata,
                    source,
                    &mut value.media_type,
                    value.title.as_deref(),
                    Kind::Document,
                    context,
                    options,
                    now,
                    resolve,
                )
                .await?
            }
            DocumentSource::Text { text } => {
                let mut source = Asset::Inline {
                    bytes: text.as_bytes().to_vec(),
                };
                let mut mime = Some("text/plain".into());
                self.prepare_one(
                    model,
                    metadata,
                    &mut source,
                    &mut mime,
                    value.title.as_deref(),
                    Kind::Document,
                    context,
                    options,
                    now,
                    resolve,
                )
                .await?;
            }
            DocumentSource::Chunks { chunks } => {
                if model.dialect.vendor() != Vendor::Claude {
                    return Err(Error::invalid(
                        "custom document chunks require Claude Messages",
                    ));
                }
                for chunk in chunks {
                    if let ChunkContent::Image(value) = &mut chunk.content {
                        self.prepare_one(
                            model,
                            &mut chunk.provider_metadata,
                            &mut value.source,
                            &mut value.media_type,
                            None,
                            Kind::Image,
                            context,
                            options,
                            now,
                            resolve,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    #[allow(clippy::too_many_arguments)]
    async fn prepare_one<F, Fut>(
        &self,
        model: &Model,
        metadata: &mut ProviderMetadata,
        source: &mut Asset,
        mime: &mut Option<String>,
        filename: Option<&str>,
        kind: Kind,
        context: Context,
        options: PreparationOptions,
        now: u64,
        resolve: &mut F,
    ) -> Result<()>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        let target = vendor(model.dialect);
        #[cfg(feature = "openai")]
        if model.dialect == Dialect::Openai && matches!(kind, Kind::Audio | Kind::Video) {
            return Err(Error::invalid(
                "Responses does not support audio/video inputs",
            ));
        }
        #[cfg(feature = "chat")]
        if model.dialect == Dialect::OpenaiChat && matches!(kind, Kind::Video) {
            return Err(Error::invalid("Chat does not support video inputs"));
        }
        validate_format(target, kind, mime.as_deref())?;
        if let Asset::Reference { reference } = source {
            match resolve(reference.clone()).await? {
                ResolvedAsset::Bytes { bytes, media_type } => {
                    if let Some(expected) = mime.as_deref()
                        && expected.split(';').next() != media_type.split(';').next()
                    {
                        return Err(Error::invalid(
                            "resolved MIME differs from the block's declared format",
                        ));
                    }
                    if mime.is_none() {
                        *mime = Some(media_type);
                    }
                    *source = Asset::Inline { bytes };
                }
                ResolvedAsset::Url { url } => *source = Asset::Url { url },
            }
        }
        if let Asset::ProviderFile { vendor: owner, .. } = source {
            if *owner != target {
                return Err(Error::invalid(
                    "provider files cannot be reused across vendors",
                ));
            }
            return Ok(());
        }
        validate_format(target, kind, mime.as_deref())?;
        if let Asset::Url { .. } = source {
            if !supports_url(model.dialect, kind, context) {
                return Err(Error::invalid(
                    "the target requires bytes for this asset; resolve the application reference to bytes",
                ));
            }
            return Ok(());
        }
        let Asset::Inline { bytes } = source else {
            return Err(Error::invalid("application asset was not resolved"));
        };
        let hard = hard_limit(target, kind).min(self.limits.max_part_bytes);
        if bytes.len() as u64 > hard {
            return Err(Error::invalid(format!(
                "asset exceeds target's {hard}-byte limit; no transcoding is performed"
            )));
        }
        let mime = mime
            .as_deref()
            .ok_or_else(|| Error::invalid("inline media requires media_type"))?;
        let hash = blake3::hash(bytes).to_hex().to_string();
        if let Some(mut upload) = uploaded_from(metadata, target)?
            && upload.source_hash == hash
            && upload.media_type == mime
            && upload.expires_at.is_none_or(|expiry| expiry > now)
        {
            #[cfg(feature = "asset-upload")]
            if upload.processing {
                upload = self.refresh_upload(model, upload).await?;
                remember(metadata, target, upload.clone())?;
            }
            if upload.processing {
                return Err(Error::invalid(
                    "Gemini file is still processing; the caller controls when to prepare again",
                ));
            }
            return Ok(());
        }
        let threshold = options.inline_bytes.unwrap_or(1024 * 1024);
        if bytes.len() as u64 <= threshold || !supports_file(model.dialect, kind, context) {
            if bytes.len() as u64 > inline_limit(target, kind) {
                return Err(Error::invalid(
                    "asset exceeds native inline limit and this content container does not accept Files IDs",
                ));
            }
            return Ok(());
        }
        #[cfg(feature = "asset-upload")]
        {
            let name = filename
                .map(str::to_owned)
                .unwrap_or_else(|| format!("asset.{}", extension(mime)));
            let upload = self.upload_asset(model, bytes, mime, &name, hash).await?;
            let processing = upload.processing;
            remember(metadata, target, upload)?;
            if processing {
                return Err(Error::invalid(
                    "Gemini upload is processing; its ID was saved and can be refreshed on the next prepare_assets call",
                ));
            }
            Ok(())
        }
        #[cfg(not(feature = "asset-upload"))]
        {
            let _ = (filename, hash);
            Err(Error::invalid(
                "asset requires Files upload; enable asset-upload",
            ))
        }
    }
}
fn validate_format(vendor: Vendor, kind: Kind, mime: Option<&str>) -> Result<()> {
    if vendor == Vendor::Claude && matches!(kind, Kind::Audio | Kind::Video | Kind::File) {
        return Err(Error::invalid(
            "Claude Messages does not support this attachment kind",
        ));
    }
    if let Some(mime) = mime {
        let base = mime.split(';').next().unwrap_or(mime);
        if matches!(kind, Kind::Image) && !base.starts_with("image/")
            || matches!(kind, Kind::Audio) && !base.starts_with("audio/")
            || matches!(kind, Kind::Video) && !base.starts_with("video/")
        {
            return Err(Error::invalid("media_type does not match the content kind"));
        }
        if matches!(kind, Kind::Image)
            && vendor == Vendor::Claude
            && !matches!(
                base,
                "image/jpeg" | "image/png" | "image/gif" | "image/webp"
            )
        {
            return Err(Error::invalid(
                "Claude images require JPEG, PNG, GIF or WebP",
            ));
        }
        if matches!(kind, Kind::Document) && !matches!(base, "application/pdf" | "text/plain") {
            return Err(Error::invalid(
                "documents require PDF or text/plain; use File for other attachments",
            ));
        }
    }
    Ok(())
}
fn hard_limit(vendor: Vendor, kind: Kind) -> u64 {
    const MB: u64 = 1024 * 1024;
    match (vendor, kind) {
        (Vendor::Claude, Kind::Image) => 5 * MB,
        (Vendor::Claude, Kind::Document) => 32 * MB,
        (Vendor::Openai, Kind::Image) => 20 * MB,
        (Vendor::Openai, Kind::Document) => 50 * MB,
        (Vendor::Gemini, Kind::Document) => 50 * MB,
        (Vendor::Gemini, _) => 2 * 1024 * MB,
        _ => 512 * MB,
    }
}
fn inline_limit(vendor: Vendor, kind: Kind) -> u64 {
    hard_limit(vendor, kind).min(if vendor == Vendor::Gemini {
        20 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    })
}
fn supports_url(dialect: Dialect, kind: Kind, context: Context) -> bool {
    if matches!(context, Context::ToolResult) && dialect.vendor() == Vendor::Gemini {
        return false;
    }
    #[cfg(feature = "chat")]
    if dialect == Dialect::OpenaiChat {
        return matches!(kind, Kind::Image);
    }
    true
}
fn supports_file(dialect: Dialect, kind: Kind, context: Context) -> bool {
    if matches!(context, Context::ToolResult) && dialect.vendor() == Vendor::Gemini {
        return false;
    }
    #[cfg(feature = "chat")]
    if dialect == Dialect::OpenaiChat {
        return matches!(kind, Kind::Document | Kind::File);
    }
    true
}
fn extension(mime: &str) -> &str {
    match mime.split(';').next().unwrap_or(mime) {
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/pcm" => "pcm",
        "video/mp4" => "mp4",
        _ => "bin",
    }
}

#[cfg(feature = "asset-upload")]
mod upload {
    use super::*;
    use http::{HeaderMap, HeaderValue, Method};
    use protocol::adapt::{JsonInvocation, files};
    use protocol::capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream,
    };
    use protocol::connection::{Bytes, MultipartPart};
    use protocol::{
        Dialect as WireDialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    };
    use std::time::Duration;

    fn capability(error: Error) -> CapabilityError {
        let kind = if matches!(error, Error::Network { .. }) {
            CapabilityErrorKind::Transport
        } else {
            CapabilityErrorKind::Invalid
        };
        CapabilityError::with_source(
            kind,
            CapabilityErrorStage::BodyTransfer,
            "SDK Files request failed",
            error,
        )
    }
    // Own model names in capability targets: native/wasm futures have the same
    // lifetime contract and the selected routing name must survive each await.
    struct OwnedTarget {
        model: String,
        dialect: WireDialect,
        operation: Operation,
    }
    struct ScopedFiles<'a, B> {
        client: &'a Client<B>,
    }
    impl<B: Backend> Upstream for ScopedFiles<'_, B> {
        type Target = OwnedTarget;
        fn send<'a>(
            &'a self,
            target: &'a OwnedTarget,
            request: WireRequest<HttpBody>,
        ) -> CapabilityFuture<'a, std::result::Result<WireResponse<HttpBody>, CapabilityError>>
        {
            send(
                self.client,
                &target.model,
                OperationKey {
                    operation: target.operation,
                    dialect: target.dialect,
                },
                request,
            )
        }
        fn connect<'a>(
            &'a self,
            _target: &'a OwnedTarget,
            _request: WireRequest<()>,
        ) -> CapabilityFuture<
            'a,
            std::result::Result<protocol::capability::UpstreamConnection, CapabilityError>,
        > {
            Box::pin(async {
                Err(CapabilityError::new(
                    CapabilityErrorKind::Unsupported,
                    CapabilityErrorStage::Start,
                    "Files use HTTP",
                ))
            })
        }
        fn limits(&self) -> CapabilityLimits {
            CapabilityLimits {
                operation_total: Duration::from_secs(300),
                stream_idle: Duration::from_secs(60),
                read_bytes: self.client.limits.max_body_bytes,
                write_bytes: self.client.limits.max_body_bytes,
                ws_frame_bytes: self.client.limits.max_value_bytes,
            }
        }
    }
    fn send<'a, B: Backend>(
        client: &'a Client<B>,
        model: &'a str,
        key: OperationKey,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, std::result::Result<WireResponse<HttpBody>, CapabilityError>> {
        let endpoint = client.backend.endpoint();
        let future = client.backend.send(key, Some(model), request);
        Box::pin(async move {
            let mut response = future.await.map_err(capability)?;
            if let Some(endpoint) = endpoint
                && let Some(value) = response.headers.get("x-goog-upload-url")
            {
                let parsed: http::Uri = value
                    .to_str()
                    .map_err(|e| capability(Error::invalid(e)))?
                    .parse()
                    .map_err(|e| capability(Error::invalid(e)))?;
                if parsed.scheme().is_some() {
                    let base: http::Uri = endpoint
                        .parse()
                        .map_err(|e| capability(Error::invalid(e)))?;
                    if parsed.scheme() != base.scheme() || parsed.authority() != base.authority() {
                        return Err(capability(Error::invalid(
                            "upload session changed gateway origin",
                        )));
                    }
                    let path = parsed
                        .path()
                        .strip_prefix(base.path().trim_end_matches('/'))
                        .filter(|path| path.starts_with('/'))
                        .ok_or_else(|| {
                            capability(Error::invalid("upload session left gateway mount"))
                        })?;
                    let scheme = base.scheme_str().ok_or_else(|| {
                        capability(Error::invalid("gateway endpoint has no scheme"))
                    })?;
                    let authority = base.authority().ok_or_else(|| {
                        capability(Error::invalid("gateway endpoint has no authority"))
                    })?;
                    let mut url = format!("{scheme}://{authority}{path}");
                    if let Some(query) = parsed.query() {
                        url.push('?');
                        url.push_str(query);
                    }
                    response.headers.insert(
                        "x-goog-upload-url",
                        HeaderValue::from_str(&url).map_err(|e| capability(Error::invalid(e)))?,
                    );
                }
            }
            Ok(response)
        })
    }
    fn key(vendor: Vendor) -> WireDialect {
        match vendor {
            Vendor::Claude => WireDialect::Claude,
            Vendor::Openai => WireDialect::OpenAi,
            Vendor::Gemini => WireDialect::Gemini,
        }
    }
    fn template(path: &str) -> WireRequest<()> {
        WireRequest {
            method: Method::POST,
            path: path.into(),
            query: None,
            headers: HeaderMap::new(),
            body: (),
        }
    }
    fn expiry(value: Option<String>) -> Result<Option<u64>> {
        value
            .map(|value| {
                time::OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339)
                    .map_err(Error::invalid)
                    .and_then(|date| date.unix_timestamp().try_into().map_err(Error::invalid))
            })
            .transpose()
    }
    fn original(error: &(dyn std::error::Error + 'static)) -> Option<Error> {
        let mut source = Some(error);
        while let Some(error) = source {
            if let Some(error) = error.downcast_ref::<Error>() {
                return Some(error.clone());
            }
            source = error.source();
        }
        None
    }
    async fn rejected(
        response: WireResponse<HttpBody>,
        limits: protocol::codec::CodecLimits,
    ) -> Result<Error> {
        let bytes = protocol::codec::read_http_body(response.body, limits).await?;
        Ok(Error::from_response(
            response.status.as_u16(),
            &response.headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }
    async fn upload_error(
        error: files::UploadError,
        limits: protocol::codec::CodecLimits,
    ) -> Error {
        match error.failure {
            files::UploadFailure::Transform(error) => original(&error).unwrap_or_else(|| {
                if error.kind() == protocol::transform::TransformErrorKind::Host {
                    Error::network(error)
                } else {
                    Error::invalid(error)
                }
            }),
            files::UploadFailure::Rejected(response) => match rejected(*response, limits).await {
                Ok(error) | Err(error) => error,
            },
        }
    }
    impl<B: Backend> Client<B> {
        pub(super) async fn upload_asset(
            &self,
            model: &Model,
            bytes: &[u8],
            mime: &str,
            name: &str,
            source_hash: String,
        ) -> Result<UploadedAsset> {
            let vendor = model.dialect.vendor();
            let upstream = ScopedFiles { client: self };
            let target = OwnedTarget {
                model: model.id.clone(),
                dialect: key(vendor),
                operation: Operation::CreateFile,
            };

            let base = UploadedAsset {
                id: String::new(),
                uri: None,
                expires_at: None,
                processing: false,
                source_hash,
                media_type: mime.into(),
            };
            if vendor == Vendor::Gemini {
                let mut file_meta = protocol::gemini::files::File::builder().build();
                file_meta.display_name = Some(Some(name.into()));
                let metadata = protocol::gemini::files::UploadFileMetadata::builder()
                    .file(Some(file_meta))
                    .build();
                let endpoint = self
                    .backend
                    .endpoint()
                    .unwrap_or("https://in-process.gproxy");
                let endpoint: http::Uri = endpoint.parse().map_err(Error::invalid)?;
                let scheme = endpoint
                    .scheme_str()
                    .ok_or_else(|| Error::invalid("gateway upload endpoint requires a scheme"))?;
                let authority = endpoint
                    .authority()
                    .ok_or_else(|| Error::invalid("gateway upload endpoint requires authority"))?;
                let origin = format!("{scheme}://{authority}");
                let mut session = match files::gemini_start_resumable(
                    &upstream,
                    &target,
                    template("/upload/v1beta/files"),
                    metadata,
                    bytes.len() as u64,
                    mime,
                    &origin,
                    self.limits,
                )
                .await
                {
                    Ok(session) => session,
                    Err(error) => return Err(upload_error(error, self.limits).await),
                };
                let progress = files::gemini_upload_chunk(
                    &upstream,
                    &target,
                    &mut session,
                    HeaderMap::new(),
                    &origin,
                    Bytes::copy_from_slice(bytes),
                    true,
                    self.limits,
                )
                .await;
                return match progress {
                    Ok(files::GeminiUploadProgress::Finalized(response)) => {
                        gemini_upload(base, response.body)
                    }
                    Ok(files::GeminiUploadProgress::Accepted(_)) => Err(Error::invalid(
                        "final Gemini chunk did not return file metadata",
                    )),
                    Err(error) => Err(upload_error(error, self.limits).await),
                };
            }
            let file = MultipartPart {
                headers: {
                    let mut headers = HeaderMap::new();
                    let name = name.replace('\\', "\\\\").replace('"', "\\\"");
                    headers.insert(
                        http::header::CONTENT_DISPOSITION,
                        HeaderValue::from_str(&format!(
                            "form-data; name=\"file\"; filename=\"{name}\""
                        ))
                        .map_err(Error::invalid)?,
                    );
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_str(mime).map_err(Error::invalid)?,
                    );
                    headers
                },
                body: HttpBody::Bytes(Bytes::copy_from_slice(bytes)),
            };
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(Error::invalid)?;
            let boundary = format!("gproxy-sdk-{:032x}", u128::from_le_bytes(random));
            let mut parts = vec![file];
            let mut request = template("/v1/files");
            if vendor == Vendor::Claude {
                request
                    .headers
                    .insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                request.headers.insert(
                    "anthropic-beta",
                    HeaderValue::from_static("files-api-2025-04-14"),
                );
            } else {
                let mut headers = HeaderMap::new();
                headers.insert(
                    http::header::CONTENT_DISPOSITION,
                    HeaderValue::from_static("form-data; name=\"purpose\""),
                );
                parts.insert(
                    0,
                    MultipartPart {
                        headers,
                        body: HttpBody::Bytes(Bytes::from_static(b"user_data")),
                    },
                );
            }
            match vendor {
                Vendor::Claude => {
                    let response = files::upload_multipart_json::<
                        _,
                        protocol::claude::files::FileMetadata,
                    >(
                        &upstream, &target, request, boundary, parts, self.limits
                    )
                    .await;
                    match response {
                        Ok(JsonInvocation::Success(response)) => Ok(UploadedAsset {
                            id: response.body.id,
                            expires_at: expiry(response.body.expires_at.flatten())?,
                            ..base
                        }),
                        Ok(JsonInvocation::Rejected(response)) => {
                            Err(rejected(response, self.limits).await?)
                        }
                        Err(error) => Err(upload_error(error, self.limits).await),
                    }
                }
                Vendor::Openai => {
                    let response = files::upload_multipart_json::<
                        _,
                        protocol::openai::files::FileObject,
                    >(
                        &upstream, &target, request, boundary, parts, self.limits
                    )
                    .await;
                    match response {
                        Ok(JsonInvocation::Success(response)) => Ok(UploadedAsset {
                            id: response.body.id,
                            expires_at: response
                                .body
                                .expires_at
                                .flatten()
                                .map(u64::try_from)
                                .transpose()
                                .map_err(Error::invalid)?,
                            ..base
                        }),
                        Ok(JsonInvocation::Rejected(response)) => {
                            Err(rejected(response, self.limits).await?)
                        }
                        Err(error) => Err(upload_error(error, self.limits).await),
                    }
                }
                Vendor::Gemini => Err(Error::invalid("Gemini uploads use resumable sessions")),
            }
        }
        pub(super) async fn refresh_upload(
            &self,
            model: &Model,
            upload: UploadedAsset,
        ) -> Result<UploadedAsset> {
            let upstream = ScopedFiles { client: self };
            let target = OwnedTarget {
                model: model.id.clone(),
                dialect: WireDialect::Gemini,
                operation: Operation::RetrieveFile,
            };
            let mut request = template("/v1beta/files");
            request.method = Method::GET;
            match files::gemini_get(&upstream, &target, request, &upload.id, self.limits).await {
                Ok(JsonInvocation::Success(response)) => gemini_file(upload, response.body),
                Ok(JsonInvocation::Rejected(response)) => {
                    Err(rejected(response, self.limits).await?)
                }
                Err(error) => Err(original(&error).unwrap_or_else(|| Error::invalid(error))),
            }
        }
    }
    fn gemini_upload(
        base: UploadedAsset,
        response: protocol::gemini::files::UploadFileResponseBody,
    ) -> Result<UploadedAsset> {
        gemini_file(
            base,
            response
                .file
                .flatten()
                .ok_or_else(|| Error::invalid("Gemini upload response has no file"))?,
        )
    }
    fn gemini_file(
        mut upload: UploadedAsset,
        file: protocol::gemini::files::File,
    ) -> Result<UploadedAsset> {
        use protocol::gemini::files::FileState;
        upload.id = file
            .name
            .flatten()
            .ok_or_else(|| Error::invalid("Gemini file has no name"))?;
        upload.uri = file.uri.flatten();
        upload.expires_at = expiry(file.expiration_time.flatten())?;
        upload.processing = matches!(file.state.flatten(), Some(FileState::Processing));
        if file.error.flatten().is_some() {
            return Err(Error::invalid("Gemini file processing failed"));
        }
        Ok(upload)
    }
}
