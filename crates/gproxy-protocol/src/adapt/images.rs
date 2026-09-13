//! Bounded image calls with explicit progress. There is no implicit retry.
//!
//! The host must own a unique operation identity and scope for each generation.
//! Resource publication is idempotent; upstream image generation is not. A new
//! ImageProgress is not permission to repeat an uncertain billed generation.
//! Dropping the future stops later calls. Completed bytes/usage and recorded
//! publication identities survive in caller-owned progress; the host can query
//! publication_status and release its owned handles explicitly. This adapter
//! never deletes resources or silently compensates a partial publication.
//!
//! All requests, source options, byte/count caps and required delivery facts are
//! checked before the first upstream send. All output images are validated
//! before the first publish. Non-2xx responses retain their original HTTP body;
//! invalid decoded 2xx responses retain the typed native response, including
//! usage, so failed output conversion does not erase billing evidence.
use crate::{
    HttpBody, WireRequest, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    capability::{
        PublicationKind, PublishedResource, ResourceAccess, ResourceMetadata, ResourceReference,
        Upstream,
    },
    codec::{self, CodecLimits},
    openai::images as o,
    transform::{
        Report, TransformError, TransformErrorKind,
        images::{
            self, ImageCallResult, ImageDialect, ImageDialectBody, ImageInput, ImageResponseFacts,
            ImageTargetModels, ResolvedImageInput,
        },
    },
};
use base64::Engine;

#[derive(Debug, Clone, Copy)]
pub struct ImageLimits {
    pub codec: CodecLimits,
    pub max_calls: usize,
    pub max_input_images: usize,
    pub max_input_bytes: u64,
    pub max_total_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_total_output_bytes: u64,
}
/// Caller-owned progress survives future cancellation. Once generation starts,
/// this object cannot be reused for generation: a dropped send may have billed
/// upstream. All completed image bytes and native usage remain available.
/// Publication operation IDs are recorded BEFORE awaiting publish; recover an
/// uncertain result with ResourceAccess::publication_status(scope, id).
#[derive(Debug)]
pub struct ImageProgress<H> {
    started: bool,
    attempted_calls: usize,
    calls: Vec<ImageCallResult>,
    publication_ids: Vec<String>,
    published: Vec<PublishedResource<H>>,
}
impl<H> Default for ImageProgress<H> {
    fn default() -> Self {
        Self {
            started: false,
            attempted_calls: 0,
            calls: Vec::new(),
            publication_ids: Vec::new(),
            published: Vec::new(),
        }
    }
}
impl<H> ImageProgress<H> {
    pub fn started(&self) -> bool {
        self.started
    }
    pub fn attempted_calls(&self) -> usize {
        self.attempted_calls
    }
    pub fn calls(&self) -> &[ImageCallResult] {
        &self.calls
    }
    pub fn publication_ids(&self) -> &[String] {
        &self.publication_ids
    }
    pub fn published(&self) -> &[PublishedResource<H>] {
        &self.published
    }
}
#[derive(Debug)]
pub enum ImageError {
    Transform(TransformError),
    Rejected(Box<WireResponse<HttpBody>>),
    InvalidResponses {
        error: TransformError,
        response:
            Box<WireResponse<crate::openai::responses::response::GenerateContentResponseBody>>,
    },
    InvalidGemini {
        error: TransformError,
        response: Box<WireResponse<crate::gemini::GenerateContentResponseBody>>,
    },
}
impl From<TransformError> for ImageError {
    fn from(v: TransformError) -> Self {
        Self::Transform(v)
    }
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transform(e)
            | Self::InvalidResponses { error: e, .. }
            | Self::InvalidGemini { error: e, .. } => e.fmt(f),
            Self::Rejected(r) => write!(f, "image upstream returned HTTP {}", r.status),
        }
    }
}
impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transform(e)
            | Self::InvalidResponses { error: e, .. }
            | Self::InvalidGemini { error: e, .. } => Some(e),
            Self::Rejected(_) => None,
        }
    }
}
#[derive(Debug)]
pub struct ImageOutcome {
    pub response: o::ImagesResponse,
    pub report: Report,
}
fn limit(context: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        context,
        "image operation exceeds configured cap",
    )
}
fn clone_request<T: Clone>(r: &WireRequest<T>) -> WireRequest<T> {
    WireRequest {
        method: r.method.clone(),
        path: r.path.clone(),
        query: r.query.clone(),
        headers: r.headers.clone(),
        body: r.body.clone(),
    }
}
fn request_fits<T: serde::Serialize>(
    r: &WireRequest<T>,
    limits: CodecLimits,
    max_write: u64,
) -> Result<(), TransformError> {
    codec::encode_json(
        &r.body,
        CodecLimits {
            max_body_bytes: limits.max_body_bytes.min(max_write),
            ..limits
        },
    )
    .map(|_| ())
    .map_err(|e| {
        TransformError::new(
            if e.kind() == codec::CodecErrorKind::Limit {
                TransformErrorKind::Limit
            } else {
                TransformErrorKind::InvalidInput
            },
            "image.request",
            e.to_string(),
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn generate<A: ResourceAccess, U: Upstream>(
    access: &A,
    scope: &A::Scope,
    upstream: &U,
    target: &U::Target,
    input: ImageInput,
    dialect: ImageDialect,
    models: &ImageTargetModels,
    facts: &ImageResponseFacts,
    limits: ImageLimits,
    progress: &mut ImageProgress<A::PublishedHandle>,
) -> Result<ImageOutcome, ImageError> {
    if progress.started {
        return Err(TransformError::new(TransformErrorKind::MissingState,"image.progress","generation already started; recover recorded publications, never replay uncertain upstream calls").into());
    }
    let (context, refs, mut report) = images::preflight(&input, dialect, models)?;
    let delivery = context
        .response_format
        .unwrap_or(facts.default_response_format);
    if context.n > limits.max_calls || refs.len() > limits.max_input_images {
        return Err(limit("image.count").into());
    }
    if facts.created < 0 {
        return Err(TransformError::shape(
            "image.created",
            "nonnegative factual Unix timestamp required",
        )
        .into());
    }
    if delivery == o::ImageResponseFormat::Url {
        if facts.operation_id.len() as u64 > limits.codec.max_value_bytes {
            return Err(limit("image.operation_id").into());
        }
        if facts.operation_id.trim().is_empty() || facts.operation_id.chars().any(char::is_control)
        {
            return Err(TransformError::shape(
                "image.operation_id",
                "nonempty control-free operation identity required",
            )
            .into());
        }
        if facts
            .publish_expires_at
            .is_none_or(|t| t <= facts.observed_at)
        {
            return Err(TransformError::shape(
                "image.publish.expires_at",
                "future expiry required before generation",
            )
            .into());
        }
    }
    let mut resolved = Vec::new();
    let mut total = 0u64;
    for reference in refs {
        if resolved
            .iter()
            .any(|r: &ResolvedImageInput| r.reference == reference)
        {
            continue;
        }
        let remaining = limits
            .max_total_input_bytes
            .checked_sub(total)
            .ok_or_else(|| limit("image.input.total"))?;
        let (image, len) = read_image(
            access,
            scope,
            &reference,
            limits.codec,
            limits.max_input_bytes.min(remaining),
            facts.observed_at,
        )
        .await?;
        total = total
            .checked_add(len)
            .ok_or_else(|| limit("image.input.total"))?;
        resolved.push(image);
    }
    let prepared = images::build_request(input, dialect, models, &resolved)?.value;
    match &prepared.body {
        ImageDialectBody::Responses(r) => {
            request_fits(r, limits.codec, upstream.limits().write_bytes)?
        }
        ImageDialectBody::Gemini(r) => {
            request_fits(r, limits.codec, upstream.limits().write_bytes)?
        }
    };
    // No upstream or publication side effect occurred during preflight.
    if limits.max_output_bytes == 0
        || limits.max_total_output_bytes == 0
        || (delivery == o::ImageResponseFormat::Url && access.limits().write_bytes == 0)
    {
        return Err(limit("image.output.bytes").into());
    }
    progress.started = true;
    let mut total_output = 0u64;
    for _ in 0..context.n {
        let cap = limits
            .max_output_bytes
            .min(if delivery == o::ImageResponseFormat::Url {
                access.limits().write_bytes
            } else {
                u64::MAX
            })
            .min(
                limits
                    .max_total_output_bytes
                    .checked_sub(total_output)
                    .ok_or_else(|| limit("image.output.total"))?,
            );
        if cap == 0 {
            return Err(limit("image.output.total").into());
        }
        progress.attempted_calls += 1;
        let result = match &prepared.body {
            ImageDialectBody::Responses(r) => {
                match invoke_json::<
                    _,
                    _,
                    crate::openai::responses::response::GenerateContentResponseBody,
                >(upstream, target, clone_request(r), limits.codec)
                .await?
                {
                    JsonInvocation::Success(r) => {
                        match images::image_response_from_responses(&r.body, &context, cap) {
                            Ok(v) => v,
                            Err(error) => {
                                return Err(ImageError::InvalidResponses {
                                    error,
                                    response: Box::new(r),
                                });
                            }
                        }
                    }
                    JsonInvocation::Rejected(r) => return Err(ImageError::Rejected(Box::new(r))),
                }
            }
            ImageDialectBody::Gemini(r) => match invoke_json::<
                _,
                _,
                crate::gemini::GenerateContentResponseBody,
            >(
                upstream, target, clone_request(r), limits.codec
            )
            .await?
            {
                JsonInvocation::Success(r) => {
                    match images::image_response_from_gemini(&r.body, &context, cap) {
                        Ok(v) => v,
                        Err(error) => {
                            return Err(ImageError::InvalidGemini {
                                error,
                                response: Box::new(r),
                            });
                        }
                    }
                }
                JsonInvocation::Rejected(r) => return Err(ImageError::Rejected(Box::new(r))),
            },
        };
        total_output = total_output
            .checked_add(result.image.bytes.len() as u64)
            .ok_or_else(|| limit("image.output.total"))?;
        report
            .diagnostics
            .extend(result.report.diagnostics.iter().cloned());
        progress.calls.push(result);
    }
    // Every output has been validated before the first publication.
    if delivery == o::ImageResponseFormat::Url
        && progress
            .calls
            .iter()
            .any(|c| c.image.bytes.len() as u64 > access.limits().write_bytes)
    {
        return Err(limit("image.publish.bytes").into());
    }
    let mut data = Vec::new();
    for (index, call) in progress.calls.iter().enumerate() {
        let mut image = o::GeneratedImage::builder().build();
        image.revised_prompt = call.image.revised_prompt.clone();
        if delivery == o::ImageResponseFormat::Url {
            // Length-prefixed identity makes the index suffix unambiguous.
            let operation = format!(
                "image:{}:{}:{}",
                facts.operation_id.len(),
                facts.operation_id,
                index
            );
            progress.publication_ids.push(operation.clone());
            let publication = access
                .publish(
                    scope,
                    &operation,
                    PublicationKind::Url,
                    ResourceMetadata {
                        mime: Some(call.image.metadata.mime().into()),
                        length: Some(call.image.bytes.len() as u64),
                        filename: None,
                        expires_at: facts.publish_expires_at,
                    },
                    HttpBody::Bytes(call.image.bytes.clone()),
                )
                .await
                .map_err(TransformError::from)?;
            // Retain even a malformed host result's handle for compensation.
            progress.published.push(publication);
            let publication = progress.published.last().unwrap();
            let ResourceReference::Url(url) = &publication.reference else {
                return Err(TransformError::invalid_result(
                    "image.publish",
                    "host returned an ID instead of URL",
                )
                .into());
            };
            if url.trim().is_empty()
                || publication.metadata.mime.as_deref() != Some(call.image.metadata.mime())
                || publication.metadata.length != Some(call.image.bytes.len() as u64)
                || publication
                    .metadata
                    .expires_at
                    .is_none_or(|t| t <= facts.observed_at)
            {
                return Err(TransformError::invalid_result(
                    "image.publish",
                    "publication metadata contradicts image or is expired",
                )
                .into());
            }
            image.url = Some(url.clone());
        } else {
            image.b64_json =
                Some(base64::engine::general_purpose::STANDARD.encode(&call.image.bytes));
        }
        data.push(image);
    }
    let first = &progress.calls[0].image.metadata;
    let format = progress
        .calls
        .iter()
        .all(|c| c.image.metadata.format == first.format)
        .then_some(first.format);
    let size =
        if progress.calls.iter().all(|c| {
            (c.image.metadata.width, c.image.metadata.height) == (first.width, first.height)
        }) {
            match (first.width, first.height) {
                (1024, 1024) => Some(o::OutputImageSize::Square),
                (1536, 1024) => Some(o::OutputImageSize::Landscape),
                (1024, 1536) => Some(o::OutputImageSize::Portrait),
                _ => None,
            }
        } else {
            None
        };
    if progress.calls.iter().any(|c| c.usage.is_some()) {
        report.changed("usage","native usage is retained per call in ImageProgress; native generic token counters cannot supply Images image/text token breakdown");
    }
    let mut response = o::ImagesResponse::builder(facts.created).data(data).build();
    response.output_format = format;
    response.size = size;
    // Quality/background are requested controls, not factual upstream results.
    Ok(ImageOutcome { response, report })
}
async fn read_image<A: ResourceAccess>(
    access: &A,
    scope: &A::Scope,
    reference: &ResourceReference,
    codec_limits: CodecLimits,
    max_bytes: u64,
    observed_at: std::time::SystemTime,
) -> Result<(ResolvedImageInput, u64), TransformError> {
    let (bytes, mime) = if let ResourceReference::Url(url) = reference {
        if let Some(data) = url.strip_prefix("data:") {
            let (mime, encoded) = data.split_once(";base64,").ok_or_else(|| {
                TransformError::shape("image.data_url", "base64 data URL required")
            })?;
            let image =
                images::decode_image(encoded, Some(mime), max_bytes).map_err(input_error)?;
            (image.bytes, image.metadata.mime().to_owned())
        } else {
            read_resource(
                access,
                scope,
                reference,
                codec_limits,
                max_bytes,
                observed_at,
            )
            .await?
        }
    } else {
        read_resource(
            access,
            scope,
            reference,
            codec_limits,
            max_bytes,
            observed_at,
        )
        .await?
    };
    let len = bytes.len() as u64;
    Ok((
        ResolvedImageInput {
            reference: reference.clone(),
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            mime_type: mime,
        },
        len,
    ))
}
async fn read_resource<A: ResourceAccess>(
    access: &A,
    scope: &A::Scope,
    reference: &ResourceReference,
    codec_limits: CodecLimits,
    max_bytes: u64,
    observed_at: std::time::SystemTime,
) -> Result<(bytes::Bytes, String), TransformError> {
    let read = access
        .read(scope, reference)
        .await
        .map_err(TransformError::from)?;
    let cap = max_bytes.min(access.limits().read_bytes);
    if read.metadata.length.is_some_and(|n| n > cap) {
        return Err(limit("image.read.length"));
    }
    if read.metadata.expires_at.is_some_and(|t| t <= observed_at) {
        return Err(TransformError::invalid_result(
            "image.read",
            "host returned expired resource",
        ));
    }
    let mime = read
        .metadata
        .mime
        .ok_or_else(|| TransformError::missing_metadata("image.mime"))?;
    let bytes = codec::read_http_body(
        read.body,
        CodecLimits {
            max_body_bytes: cap,
            ..codec_limits
        },
    )
    .await
    .map_err(|e| {
        TransformError::new(
            if e.kind() == codec::CodecErrorKind::Limit {
                TransformErrorKind::Limit
            } else {
                TransformErrorKind::Host
            },
            "image.read",
            e.to_string(),
        )
    })?;
    if read
        .metadata
        .length
        .is_some_and(|n| n != bytes.len() as u64)
    {
        return Err(TransformError::invalid_result(
            "image.read.length",
            "metadata length differs from actual bytes",
        ));
    }
    let metadata = images::inspect_image(&bytes)?;
    if metadata.mime() != mime {
        return Err(TransformError::invalid_result(
            "image.read.mime",
            "MIME differs from actual image bytes",
        ));
    }
    Ok((bytes, mime))
}

fn input_error(error: TransformError) -> TransformError {
    TransformError::with_source(
        if error.kind() == TransformErrorKind::InvalidResult {
            TransformErrorKind::InvalidInput
        } else {
            error.kind()
        },
        "image.input",
        error.to_string(),
        error,
    )
}
