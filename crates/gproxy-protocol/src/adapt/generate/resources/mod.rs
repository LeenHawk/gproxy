//! Materialize declared foreign URLs/native IDs through scoped ResourceAccess.
//! Bytes and factual MIME stay in concrete vendor fields; no content IR is used.

mod chat;
mod claude;
mod gemini;
mod responses;
use crate::{
    Dialect,
    capability::{ResourceAccess, ResourceReference},
    codec::{self, CodecLimits},
    transform::{TransformError, TransformErrorKind},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::time::SystemTime;

pub struct GenerationResources<'a, R: ResourceAccess> {
    pub access: &'a R,
    pub scope: &'a R::Scope,
    pub limits: CodecLimits,
    pub max_references: usize,
    pub now: SystemTime,
    /// The dialect the request is converted into. A reference that dialect
    /// reads as it stands is left in place rather than fetched.
    pub target: Dialect,
}

/// Whether a media reference converted from `source` reaches `target` as a
/// reference the upstream reads itself, so no bytes need fetching. Only
/// URLs to dialects with a URL form for that media, and file IDs between the
/// OpenAI dialects, survive conversion; every other reference is fetched.
/// Tests pin this against what the direct transforms actually keep.
pub fn reference_passes_through(
    source: Dialect,
    target: Dialect,
    image: bool,
    reference: &ResourceReference,
) -> bool {
    let target = target.pair_dialect();
    match reference {
        ResourceReference::Url(_) if image => target != Dialect::Gemini,
        ResourceReference::Url(_) => matches!(target, Dialect::OpenAi | Dialect::Claude),
        ResourceReference::Id(_) => {
            !image
                && source.family() == crate::WireFamily::OpenAi
                && target.family() == crate::WireFamily::OpenAi
        }
        #[allow(unreachable_patterns)]
        _ => false,
    }
}

pub(super) struct Budget<'a, 'b, R: ResourceAccess> {
    ctx: &'a GenerationResources<'b, R>,
    source: Dialect,
    remaining: u64,
    references: usize,
}

struct Media {
    bytes: bytes::Bytes,
    mime: String,
    filename: Option<String>,
}

impl Media {
    fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, STANDARD.encode(&self.bytes))
    }
}

impl<R: ResourceAccess> GenerationResources<'_, R> {
    fn budget(&self, source: Dialect) -> Budget<'_, '_, R> {
        Budget {
            ctx: self,
            source,
            remaining: self
                .limits
                .max_body_bytes
                .min(self.access.limits().read_bytes),
            references: 0,
        }
    }
}

impl<R: ResourceAccess> Budget<'_, '_, R> {
    /// Fetch a reference the target cannot read, or `None` to leave it.
    async fn read(
        &mut self,
        reference: ResourceReference,
        image: bool,
    ) -> Result<Option<Media>, TransformError> {
        if reference_passes_through(self.source, self.ctx.target, image, &reference) {
            return Ok(None);
        }
        if self.references >= self.ctx.max_references || self.remaining == 0 {
            return Err(limit());
        }
        self.references += 1;
        let result = self.ctx.access.read(self.ctx.scope, &reference).await?;
        if result
            .metadata
            .length
            .is_some_and(|length| length > self.remaining)
        {
            return Err(limit());
        }
        if result
            .metadata
            .expires_at
            .is_some_and(|expiry| expiry <= self.ctx.now)
        {
            return Err(TransformError::invalid_result(
                "resource.expiry",
                "resource is expired",
            ));
        }
        let mime = result
            .metadata
            .mime
            .ok_or_else(|| TransformError::missing_metadata("resource.mime"))?;
        if mime.trim().is_empty() || mime.contains([';', ',', '\r', '\n']) {
            return Err(TransformError::invalid_result(
                "resource.mime",
                "exact MIME type required",
            ));
        }
        let mut limits = self.ctx.limits;
        limits.max_body_bytes = limits.max_body_bytes.min(self.remaining);
        limits.max_buffer_bytes = limits.max_buffer_bytes.min(self.remaining);
        let bytes = codec::read_http_body(result.body, limits)
            .await
            .map_err(|e| {
                TransformError::new(
                    if e.kind() == codec::CodecErrorKind::Limit {
                        TransformErrorKind::Limit
                    } else {
                        TransformErrorKind::Host
                    },
                    "resource.body",
                    e.to_string(),
                )
            })?;
        if result
            .metadata
            .length
            .is_some_and(|n| n != bytes.len() as u64)
        {
            return Err(TransformError::invalid_result(
                "resource.length",
                "declared length differs from actual bytes",
            ));
        }
        self.remaining = self
            .remaining
            .checked_sub(bytes.len() as u64)
            .ok_or_else(limit)?;
        if (image || matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/webp"))
            && crate::transform::images::inspect_image(&bytes)?.mime() != mime
        {
            return Err(TransformError::invalid_result(
                "resource.mime",
                "image bytes differ from MIME",
            ));
        }
        Ok(Some(Media {
            bytes,
            mime,
            filename: result.metadata.filename,
        }))
    }
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.resources",
        "resource count or aggregate byte budget exceeded",
    )
}
