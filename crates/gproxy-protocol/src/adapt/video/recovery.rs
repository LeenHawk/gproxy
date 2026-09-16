use super::*;
use crate::capability::{ResourceAccess, StateStore};
use std::time::SystemTime;

/// Complete binding/publication from the actual retained result of this call.
/// This never issues another upstream create or guesses an unknown operation.
#[allow(clippy::too_many_arguments)]
pub async fn recover_created_result<R: ResourceAccess, S: StateStore>(
    access: &R,
    resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    binding: VideoBinding,
    expires_at: SystemTime,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    if expires_at <= SystemTime::now() {
        return Err(TransformError::shape(
            "video.expiry",
            "future publication expiry required",
        ));
    }
    if progress.binding.as_ref() != Some(&binding) || !progress.send_started {
        return Err(TransformError::shape(
            "video.recovery.binding",
            "progress belongs to a different invocation",
        ));
    }
    let native = progress
        .native_response
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("video.recovery.native_response"))?;
    let response = crate::WireResponse {
        status: native.status,
        headers: native.headers.clone(),
        body: native.body.clone(),
    };
    super::composed::operation(&response.body, &binding)?;
    let (mut state, version) = state::load(store, state_scope, &binding, limits).await?;
    if !matches!(state.original, VideoOriginalRequest::OpenRouter(_)) {
        return Err(TransformError::shape(
            "video.source",
            "native job requires native recovery projection",
        ));
    }
    if state
        .operation
        .as_ref()
        .is_some_and(|old| old != &response.body)
    {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "video.recovery.operation",
            "durable operation differs from retained result",
        ));
    }
    state.operation = Some(response.body.clone());
    let version = state::save(store, state_scope, &state, Some(version), limits).await?;
    progress.binding_saved = true;
    super::composed::finish(
        access,
        resource_scope,
        store,
        state_scope,
        response,
        &mut state,
        version,
        expires_at,
        progress,
        limits,
    )
    .await
}
