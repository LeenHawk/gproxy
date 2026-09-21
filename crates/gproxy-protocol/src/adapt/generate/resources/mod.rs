//! Materialize declared foreign URLs/native IDs through scoped ResourceAccess.
//! Bytes and factual MIME stay in concrete vendor fields; no content IR is used.

mod chat;
mod claude;
mod gemini;
mod responses;
use crate::{
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
}

pub(super) struct Budget<'a, 'b, R: ResourceAccess> {
    ctx: &'a GenerationResources<'b, R>,
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
    fn budget(&self) -> Budget<'_, '_, R> {
        Budget {
            ctx: self,
            remaining: self
                .limits
                .max_body_bytes
                .min(self.access.limits().read_bytes),
            references: 0,
        }
    }
}

impl<R: ResourceAccess> Budget<'_, '_, R> {
    async fn read(
        &mut self,
        reference: ResourceReference,
        image: bool,
    ) -> Result<Media, TransformError> {
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
        Ok(Media {
            bytes,
            mime,
            filename: result.metadata.filename,
        })
    }
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.resources",
        "resource count or aggregate byte budget exceeded",
    )
}
