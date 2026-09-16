use super::*;
use super::{resources::*, state::*};
use crate::{
    HttpBody, WireResponse,
    capability::{ResourceAccess, StateStore},
    transform::TransformErrorKind,
};
use std::time::SystemTime;

pub(super) fn fresh(progress: &VideoProgress) -> Result<(), TransformError> {
    if progress.send_started || progress.reserved || progress.raw_response.is_some() {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "video.progress",
            "invocation progress already used; inspect the saved result instead of recreating",
        ));
    }
    Ok(())
}
pub(super) fn prepare_headers(
    template: WireRequest<()>,
    body: bytes::Bytes,
    json: bool,
) -> WireRequest<HttpBody> {
    let mut headers = template.headers;
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
        http::header::CONTENT_TYPE,
    ] {
        headers.remove(name);
    }
    if json {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
    }
    WireRequest {
        method: template.method,
        path: template.path,
        query: template.query,
        headers,
        body: HttpBody::Bytes(body),
    }
}
pub(super) fn operation<'a>(
    body: &'a g::VideoOperation,
    binding: &VideoBinding,
) -> Result<&'a str, TransformError> {
    let name = body
        .name
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("video.operation.name"))?;
    binding.query_path(name)?;
    if body.error.is_some() && body.response.is_some()
        || body.done != Some(true) && (body.error.is_some() || body.response.is_some())
    {
        return Err(TransformError::invalid_result(
            "video.operation",
            "inconsistent terminal operation fields",
        ));
    }
    Ok(name)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn finish<R: ResourceAccess, S: StateStore>(
    access: &R,
    resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    mut response: WireResponse<g::VideoOperation>,
    state: &mut VideoJobState,
    version: crate::capability::Version,
    expires_at: SystemTime,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    let resources = outputs(
        access,
        resource_scope,
        &response.body,
        &state.binding,
        expires_at,
        progress,
        limits,
    )
    .await?;
    for (key, value) in &resources {
        state
            .published_urls
            .insert(key.clone(), value.url.clone().expect("published"));
    }
    // Rewrite only declared URI fields on a projection; the original native
    // operation remains in progress and durable state for recovery.
    if let Some(samples) = response
        .body
        .response
        .as_mut()
        .and_then(|r| r.generate_video_response.as_mut())
        .and_then(|r| r.generated_samples.as_mut())
    {
        for sample in samples {
            if let Some(video) = &mut sample.video
                && let Some(uri) = &video.uri
            {
                video.uri = Some(
                    resources
                        .get(uri)
                        .and_then(|r| r.url.clone())
                        .ok_or_else(|| TransformError::missing_metadata("video.published_uri"))?,
                );
            }
        }
    }
    let name = operation(&response.body, &state.binding)?.to_owned();
    let mut converted = video::gemini_operation_to_openai_response(
        response.body,
        &OpenAiVideoResponseContext {
            operation_name: name,
            polling_url: state.binding.polling_url.clone(),
            resources,
        },
    )?;
    converted.value.id = state.binding.client_id.clone();
    bound_value(&converted.value, limits)?;
    // Publication bindings must be durable before URLs are exposed.
    save(store, state_scope, state, Some(version), limits).await?;
    progress.binding_saved = true;
    Ok(JsonInvocation::Success(WireResponse {
        status: response.status,
        headers: response.headers,
        body: converted,
    }))
}
#[allow(clippy::too_many_arguments)]
pub async fn openai_to_gemini_create_composed<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    template: WireRequest<()>,
    input: o::CreateVideoRequestBody,
    binding: VideoBinding,
    expires_at: SystemTime,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    fresh(progress)?;

    direct::check_template(&template, http::Method::POST)?;
    if template.path != binding.create_path()? {
        return Err(TransformError::shape(
            "video.create.path",
            "path differs from selected Veo model/API prefix",
        ));
    }
    if expires_at <= SystemTime::now() {
        return Err(TransformError::shape(
            "video.expiry",
            "future publication expiry required",
        ));
    }
    let input = input.into_declared();
    bound_value(&input, limits)?;
    bound_value(&binding, limits)?;
    let resources = resolve_resources(access, resource_scope, &input, limits).await?;
    let prepared = video::openai_to_gemini_request(input.clone(), &binding.model, &resources)?;
    let mut codec = limits.codec;
    codec.max_body_bytes = codec.max_body_bytes.min(upstream.limits().write_bytes);
    codec.max_buffer_bytes = codec.max_buffer_bytes.min(codec.max_body_bytes);
    codec.max_value_bytes = codec.max_value_bytes.min(codec.max_body_bytes);
    let bytes = crate::codec::encode_json(&prepared.value.body, codec).map_err(|e| {
        TransformError::new(TransformErrorKind::Limit, "video.request", e.to_string())
    })?;
    let mut state = VideoJobState {
        schema: 1,
        binding,
        original: VideoOriginalRequest::OpenRouter(input),
        native_facts: None,
        effective: prepared.value.body,
        operation: None,
        published_urls: BTreeMap::new(),
    };
    progress.binding = Some(state.binding.clone());
    let version = save(store, state_scope, &state, None, limits).await?;
    progress.reserved = true;
    let response = super::transport::invoke(
        upstream,
        target,
        prepare_headers(template, bytes, true),
        &mut progress.send_started,
        &mut progress.raw_response,
        &mut progress.native_response,
        limits,
    )
    .await?;
    let JsonInvocation::Success(response) = response else {
        return Ok(match response {
            JsonInvocation::Rejected(r) => JsonInvocation::Rejected(r),
            _ => unreachable!(),
        });
    };
    operation(&response.body, &state.binding)?;
    state.operation = Some(response.body.clone());
    let version = save(store, state_scope, &state, Some(version), limits).await?;
    progress.binding_saved = true;
    finish(
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
#[allow(clippy::too_many_arguments)]
pub async fn openai_to_gemini_query_composed<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    template: WireRequest<()>,
    binding: VideoBinding,
    expires_at: SystemTime,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    fresh(progress)?;

    direct::check_template(&template, http::Method::GET)?;
    if expires_at <= SystemTime::now() {
        return Err(TransformError::shape(
            "video.expiry",
            "future publication expiry required",
        ));
    }
    let (mut state, version) = load(store, state_scope, &binding, limits).await?;
    if !matches!(state.original, VideoOriginalRequest::OpenRouter(_)) {
        return Err(TransformError::shape(
            "video.source",
            "native job requires native query projection",
        ));
    }
    let old = state.operation.as_ref().ok_or_else(|| {
        TransformError::new(
            TransformErrorKind::Conflict,
            "video.creation",
            "creation outcome uncertain; caller must reconcile the existing operation",
        )
    })?;
    let name = operation(old, &binding)?.to_owned();
    if template.path != binding.query_path(&name)? {
        return Err(TransformError::shape(
            "video.query.path",
            "query path differs from saved operation",
        ));
    }
    progress.binding = Some(binding.clone());
    let response = super::transport::invoke(
        upstream,
        target,
        prepare_headers(template, bytes::Bytes::new(), false),
        &mut progress.send_started,
        &mut progress.raw_response,
        &mut progress.native_response,
        limits,
    )
    .await?;
    let JsonInvocation::Success(response) = response else {
        return Ok(match response {
            JsonInvocation::Rejected(r) => JsonInvocation::Rejected(r),
            _ => unreachable!(),
        });
    };
    if operation(&response.body, &binding)? != name {
        return Err(TransformError::invalid_result(
            "video.query.identity",
            "upstream returned a different operation",
        ));
    }
    if old.done == Some(true)
        && (old.done != response.body.done
            || old.response != response.body.response
            || old.error != response.body.error)
    {
        return Err(TransformError::invalid_result(
            "video.query.lifecycle",
            "terminal operation changed",
        ));
    }
    state.operation = Some(response.body.clone());
    let version = save(store, state_scope, &state, Some(version), limits).await?;
    progress.binding_saved = true;
    finish(
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
