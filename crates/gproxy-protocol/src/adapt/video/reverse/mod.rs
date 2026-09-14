//! Veo instance/sample fanout into concrete OpenRouter or native Sora jobs.
mod invoke;
mod output;
mod prepare;
mod state;
use super::*;
use crate::{
    WireResponse,
    capability::{ResourceAccess, ResourceReference, StateStore, Version},
    transform::{Report, TransformErrorKind},
};
pub use invoke::{
    gemini_video_create_composed, gemini_video_query_composed, gemini_video_resume_composed,
    recover_reverse_video_result,
};
use state::*;
pub use state::{
    ReverseVideoBinding, ReverseVideoChild, ReverseVideoKind, ReverseVideoProgress,
    ReverseVideoRequest, ReverseVideoResult, ReverseVideoState,
};
use std::time::SystemTime;

async fn publish<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    id: &str,
    bytes: bytes::Bytes,
    mime: &str,
    expires_at: SystemTime,
) -> Result<crate::capability::PublishedResource<R::PublishedHandle>, TransformError> {
    let value = super::resources::publish(access, scope, id, bytes, mime, expires_at).await?;
    if value.metadata.expires_at.is_none_or(|e| e < expires_at) {
        return Err(TransformError::invalid_result(
            "video.publication.expiry",
            "publication expires before the saved group binding",
        ));
    }
    Ok(value)
}
