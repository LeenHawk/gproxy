//! Explicit image reads and publication with caller-owned cancellation progress.
//! The native Gemini response is kept apart from the view its images are read or
//! published into; an image signature travels with the view to the client.

mod publish;
mod read;
mod stream;
mod thread;
use super::GenerationProgress;
use crate::{
    capability::PublishedResource,
    transform::{TransformError, TransformErrorKind},
    wire::gemini as g,
};
use std::collections::BTreeMap;
pub use stream::ImageStreamProgress;
pub use thread::{ResourceSend, ResourceSync};

#[derive(Debug, Clone)]
pub struct MaterializedImage {
    pub candidate: usize,
    pub part: usize,
    pub original: g::Part,
    pub materialized: g::Blob,
    /// Expiry observed while reading the original resource, if the host knows it.
    pub resource_expiry: Option<std::time::SystemTime>,
}

/// Retains completed reads and an active resource body. Bytes are counted as
/// each chunk arrives; cancellation resumes that same body without a new read.
#[derive(Default)]
pub struct GeminiImageReads {
    original: Option<g::GenerateContentResponseBody>,
    images: BTreeMap<(usize, usize), MaterializedImage>,
    attempted: usize,
    bytes: u64,
    pending: Option<PendingImageRead>,
    failed: bool,
}

impl GeminiImageReads {
    pub fn bytes_read(&self) -> u64 {
        self.bytes
    }
    pub fn attempted_reads(&self) -> usize {
        self.attempted
    }
    pub fn images(&self) -> impl Iterator<Item = &MaterializedImage> {
        self.images.values()
    }
}

pub(super) struct Publication {
    source: g::Blob,
    operation: String,
    expiry: std::time::SystemTime,
    receipt: Option<usize>,
}

/// Receipts, including malformed host replies, remain inspectable for host
/// compensation. An uncertain publication is queried, never automatically sent
/// again. This object and its resource scope belong to one invocation.
pub struct GeminiImagePublications<H> {
    original: Option<g::GenerateContentResponseBody>,
    publications: BTreeMap<(usize, usize), Publication>,
    receipts: Vec<PublishedResource<H>>,
    failed: bool,
}

impl<H> Default for GeminiImagePublications<H> {
    fn default() -> Self {
        Self {
            original: None,
            publications: BTreeMap::new(),
            receipts: Vec::new(),
            failed: false,
        }
    }
}

impl<H> GeminiImagePublications<H> {
    pub fn operation_ids(&self) -> impl Iterator<Item = &str> {
        self.publications
            .values()
            .map(|value| value.operation.as_str())
    }
    pub fn receipts(&self) -> &[PublishedResource<H>] {
        &self.receipts
    }
}

/// Generation and resource work have independent retained progress.
/// Re-enter through `recover_with_image_resources`, never repeat an uncertain POST.
pub struct ImageResourceProgress<N, H> {
    pub generation: GenerationProgress<N>,
    pub reads: GeminiImageReads,
    pub publications: GeminiImagePublications<H>,
}

impl<N, H> Default for ImageResourceProgress<N, H> {
    fn default() -> Self {
        Self {
            generation: Default::default(),
            reads: Default::default(),
            publications: Default::default(),
        }
    }
}

fn invalid(message: impl Into<String>) -> TransformError {
    TransformError::invalid_result("generation.image_resources", message)
}

fn missing(message: impl Into<String>) -> TransformError {
    TransformError::new(
        TransformErrorKind::MissingState,
        "generation.image_resources",
        message,
    )
}

fn conflict(message: impl Into<String>) -> TransformError {
    TransformError::new(
        TransformErrorKind::Conflict,
        "generation.image_resources",
        message,
    )
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.image_resources",
        "image resource budget exceeded",
    )
}

pub(crate) fn wants_uri(input: &g::GenerateContentRequestBody) -> bool {
    input
        .generation_config
        .as_ref()
        .and_then(|config| config.response_format.as_ref())
        .and_then(|format| format.image.as_ref())
        .and_then(|image| image.delivery.as_ref())
        == Some(&g::Delivery::Uri)
}

struct PendingImageRead {
    key: (usize, usize),
    reference: crate::capability::ResourceReference,
    metadata: crate::capability::ResourceMetadata,
    body: crate::HttpBody,
    buffer: Vec<u8>,
    limit: u64,
}

fn codec_error(error: crate::codec::CodecError) -> TransformError {
    let kind = if error.kind() == crate::codec::CodecErrorKind::Limit {
        TransformErrorKind::Limit
    } else {
        TransformErrorKind::InvalidResult
    };
    TransformError::with_source(kind, "generation.image_resources", error.to_string(), error)
}
