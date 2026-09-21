use super::super::{composed::prepare_headers, direct::check_template, resources::bound_value};
use super::*;

fn expiry_valid(expiry: SystemTime) -> Result<(), TransformError> {
    if expiry <= SystemTime::now() {
        return Err(TransformError::shape(
            "video.expiry",
            "future publication expiry required",
        ));
    }
    Ok(())
}

fn template(
    binding: &ReverseVideoBinding,
    mut template: WireRequest<()>,
    method: http::Method,
    id: Option<&str>,
) -> Result<WireRequest<()>, TransformError> {
    check_template(&template, method.clone())?;
    let path = if let Some(id) = id {
        binding.query_path(id)?
    } else {
        binding.create_path.clone()
    };
    if template.path != path {
        return Err(TransformError::shape(
            "video.template.path",
            "path differs from saved target API binding",
        ));
    }
    template.method = method;
    Ok(template)
}

#[allow(clippy::too_many_arguments)]
async fn send<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    request: Option<&ReverseVideoRequest>,
    kind: ReverseVideoKind,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<ReverseVideoResult>, TransformError> {
    let mut codec = limits.codec;
    codec.max_body_bytes = codec.max_body_bytes.min(upstream.limits().write_bytes);
    codec.max_buffer_bytes = codec.max_buffer_bytes.min(codec.max_body_bytes);
    codec.max_value_bytes = codec.max_value_bytes.min(codec.max_body_bytes);
    let bytes = match request {
        Some(ReverseVideoRequest::Native(v)) => crate::codec::encode_json(v, codec),
        Some(ReverseVideoRequest::OpenRouter(v)) => crate::codec::encode_json(v, codec),
        None => Ok(bytes::Bytes::new()),
    }
    .map_err(|e| TransformError::new(TransformErrorKind::Limit, "video.request", e.to_string()))?;
    let request = prepare_headers(template, bytes, request.is_some());
    progress.raw_response = None;
    progress.result = None;
    macro_rules! invoke {
        ($ty:ty,$variant:ident) => {{
            let mut native = None;
            let response = super::super::transport::invoke::<_, $ty>(
                upstream,
                target,
                request,
                &mut progress.send_started,
                &mut progress.raw_response,
                &mut native,
                limits,
            )
            .await;
            progress.result = native.map(|r| ReverseVideoResult::$variant(r.body));
            match response? {
                JsonInvocation::Rejected(r) => Ok(JsonInvocation::Rejected(r)),
                JsonInvocation::Success(r) => Ok(JsonInvocation::Success(WireResponse {
                    status: r.status,
                    headers: r.headers,
                    body: ReverseVideoResult::$variant(r.body),
                })),
            }
        }};
    }
    match kind {
        ReverseVideoKind::Native => invoke!(o::NativeVideo, Native),
        ReverseVideoKind::OpenRouter => invoke!(o::VideoGenerationResponseBody, OpenRouter),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn gemini_video_create_composed<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    source_scope: &R::Scope,
    target_scope: &R::Scope,
    publish_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    create_template: WireRequest<()>,
    input: g::PredictLongRunningRequestBody,
    binding: ReverseVideoBinding,
    effective_samples: Option<usize>,
    defaults: Option<video::NativeVideoDefaults>,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    if progress.state.is_some() || progress.send_started {
        return Err(conflict("video.progress"));
    }
    expiry_valid(expiry)?;
    let state = prepare::split(
        input,
        binding,
        effective_samples,
        defaults.clone(),
        expiry,
        limits,
    )?;
    template(
        &state.binding,
        copy_template(&create_template),
        http::Method::POST,
        None,
    )?;
    prepare::controls(&state, defaults.clone())?;
    save(store, state_scope, &state, progress, limits).await?;
    drive(
        upstream,
        target,
        access,
        source_scope,
        target_scope,
        publish_scope,
        store,
        state_scope,
        create_template,
        state,
        defaults,
        expiry,
        progress,
        limits,
    )
    .await
}

/// Continue only children whose durable `started` marker is false. A started
/// child without a retained result requires explicit caller reconciliation.
#[allow(clippy::too_many_arguments)]
pub async fn gemini_video_resume_composed<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    source_scope: &R::Scope,
    target_scope: &R::Scope,
    publish_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    create_template: WireRequest<()>,
    binding: ReverseVideoBinding,
    defaults: Option<video::NativeVideoDefaults>,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    expiry_valid(expiry)?;
    let (state, version) = load(store, state_scope, &binding, limits).await?;
    if state.native_defaults != defaults || state.expires_at != expiry {
        return Err(TransformError::shape(
            "video.resume.facts",
            "defaults and publication expiry differ from saved invocation",
        ));
    }
    template(
        &binding,
        copy_template(&create_template),
        http::Method::POST,
        None,
    )?;
    prepare::controls(&state, defaults.clone())?;
    progress.version = Some(version);
    progress.state = Some(state.clone());
    drive(
        upstream,
        target,
        access,
        source_scope,
        target_scope,
        publish_scope,
        store,
        state_scope,
        create_template,
        state,
        defaults,
        expiry,
        progress,
        limits,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn drive<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    source_scope: &R::Scope,
    target_scope: &R::Scope,
    publish_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    create_template: WireRequest<()>,
    mut state: ReverseVideoState,
    defaults: Option<video::NativeVideoDefaults>,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    let mut remaining = limits.codec.max_body_bytes;
    for index in 0..state.children.len() {
        if state.children[index].result.is_some() {
            continue;
        }
        if state.children[index].started {
            return Err(conflict("video.child.creation"));
        }
        progress.child_index = Some(index);
        if state.children[index].request.is_none() {
            state.children[index].request = Some(
                prepare::prepare(
                    access,
                    source_scope,
                    publish_scope,
                    &state,
                    index,
                    defaults.clone(),
                    expiry,
                    progress,
                    limits,
                    &mut remaining,
                )
                .await?,
            );
            save(store, state_scope, &state, progress, limits).await?;
        }
        // Local encoding/write limits must fail before the durable send marker.
        let mut preflight = limits.codec;
        preflight.max_body_bytes = preflight.max_body_bytes.min(upstream.limits().write_bytes);
        preflight.max_buffer_bytes = preflight.max_buffer_bytes.min(preflight.max_body_bytes);
        preflight.max_value_bytes = preflight.max_value_bytes.min(preflight.max_body_bytes);
        match state.children[index].request.as_ref().expect("prepared") {
            ReverseVideoRequest::Native(v) => crate::codec::encode_json(v, preflight),
            ReverseVideoRequest::OpenRouter(v) => crate::codec::encode_json(v, preflight),
        }
        .map_err(|e| {
            TransformError::new(TransformErrorKind::Limit, "video.request", e.to_string())
        })?;
        state.children[index].started = true;
        save(store, state_scope, &state, progress, limits).await?;
        let response = send(
            upstream,
            target,
            copy_template(&create_template),
            state.children[index].request.as_ref(),
            state.binding.kind,
            progress,
            limits,
        )
        .await?;
        let response = match response {
            JsonInvocation::Success(response) => response,
            JsonInvocation::Rejected(r) => return Ok(JsonInvocation::Rejected(r)),
        };

        state.children[index].result = Some(response.body);
        save(store, state_scope, &state, progress, limits).await?;
    }
    let body = output::finish(
        access,
        target_scope,
        publish_scope,
        store,
        state_scope,
        &mut state,
        expiry,
        progress,
        limits,
    )
    .await?;
    Ok(JsonInvocation::Success(WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body,
    }))
}

/// Poll a selected child using its exact recorded resource path. Other children
/// retain their actual outcomes and are never implicitly recreated or polled.
#[allow(clippy::too_many_arguments)]
pub async fn gemini_video_query_composed<U: Upstream, R: ResourceAccess, S: StateStore>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    target_scope: &R::Scope,
    publish_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    query_template: WireRequest<()>,
    binding: ReverseVideoBinding,
    index: usize,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    expiry_valid(expiry)?;
    let (mut state, version) = load(store, state_scope, &binding, limits).await?;
    if state.expires_at != expiry {
        return Err(TransformError::shape(
            "video.query.expiry",
            "publication expiry differs from saved invocation",
        ));
    }
    let child = state
        .children
        .get(index)
        .ok_or_else(|| TransformError::shape("video.child", "index out of range"))?;
    let old = child
        .result
        .as_ref()
        .ok_or_else(|| conflict("video.child.creation"))?;
    let query_template = template(&binding, query_template, http::Method::GET, Some(old.id()))?;
    progress.version = Some(version);
    progress.state = Some(state.clone());
    progress.child_index = Some(index);
    let response = send(
        upstream,
        target,
        query_template,
        None,
        binding.kind,
        progress,
        limits,
    )
    .await?;
    let response = match response {
        JsonInvocation::Success(response) => response,
        JsonInvocation::Rejected(r) => return Ok(JsonInvocation::Rejected(r)),
    };

    if state.children[index]
        .result
        .as_ref()
        .is_none_or(|old| !old.same_projection(&response.body))
    {
        state.children[index].published = None;
    }
    state.children[index].result = Some(response.body);
    save(store, state_scope, &state, progress, limits).await?;
    let body = output::finish(
        access,
        target_scope,
        publish_scope,
        store,
        state_scope,
        &mut state,
        expiry,
        progress,
        limits,
    )
    .await?;
    Ok(JsonInvocation::Success(WireResponse {
        status: response.status,
        headers: response.headers,
        body,
    }))
}

/// Persist a retained, real result after a post-send CAS failure. This performs
/// no upstream request; unknown outcomes need caller reconciliation first.
pub async fn recover_reverse_video_result<S: StateStore>(
    store: &S,
    scope: &S::Scope,
    binding: &ReverseVideoBinding,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<(), TransformError> {
    let index = progress
        .child_index
        .ok_or_else(|| TransformError::missing_metadata("video.progress.child"))?;
    let result = progress
        .result
        .clone()
        .ok_or_else(|| TransformError::missing_metadata("video.progress.result"))?;
    if progress
        .state
        .as_ref()
        .is_none_or(|s| &s.binding != binding)
    {
        return Err(TransformError::shape(
            "video.progress.binding",
            "different saved invocation",
        ));
    }
    let (mut state, version) = load(store, scope, binding, limits).await?;
    if state.children.get(index).is_none_or(|c| !c.started) {
        return Err(conflict("video.child.reservation"));
    }

    state.children[index].result = Some(result);
    state.children[index].published = None;
    bound_value(&state, limits)?;
    progress.version = Some(version);
    save(store, scope, &state, progress, limits).await
}

fn copy_template(request: &WireRequest<()>) -> WireRequest<()> {
    WireRequest {
        method: request.method.clone(),
        path: request.path.clone(),
        query: request.query.clone(),
        headers: request.headers.clone(),
        body: (),
    }
}
