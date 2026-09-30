use super::*;
use super::{
    composed::{operation, prepare_headers},
    resources::*,
    state::*,
};
use crate::{
    HttpBody, WireResponse,
    capability::{
        ResourceAccess, ResourceMetadata, ResourceRead, ResourceReference, StateStore, Version,
    },
    transform::{
        TransformErrorKind,
        video::{NativeVideoDefaults, NativeVideoResponseFacts},
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

async fn input_resources<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    input: &o::NativeCreateVideoRequestBody,
    limits: VideoLimits,
) -> Result<BTreeMap<String, ResolvedVideoResource>, TransformError> {
    let mut out = BTreeMap::new();
    let Some(reference) = &input.input_reference else {
        return Ok(out);
    };

    let (key, reference) = match reference {
        o::NativeInputReference::File(v) => {
            (v.file_id.clone(), ResourceReference::Id(v.file_id.clone()))
        }
        o::NativeInputReference::Url(v) => (
            v.image_url.clone(),
            ResourceReference::Url(v.image_url.clone()),
        ),
    };
    let (bytes, mime) = read(
        access,
        scope,
        &reference,
        true,
        limits,
        limits.codec.max_body_bytes,
    )
    .await?;
    out.insert(
        key.clone(),
        ResolvedVideoResource {
            reference: key,
            url: None,
            bytes_base64_encoded: Some(STANDARD.encode(bytes)),
            mime_type: Some(mime),
        },
    );
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub async fn native_to_gemini_create_composed<
    U: Upstream,
    R: ResourceAccess,
    S: StateStore,
    F: FnOnce(&g::VideoOperation) -> Result<NativeVideoResponseFacts, TransformError>,
>(
    upstream: &U,
    target: &U::Target,
    access: &R,
    source_resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    template: WireRequest<()>,
    input: o::NativeCreateVideoRequestBody,
    defaults: Option<NativeVideoDefaults>,
    binding: VideoBinding,
    facts: F,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::NativeVideo>>, TransformError> {
    super::composed::fresh(progress)?;

    direct::check_template(&template, http::Method::POST)?;
    if template.path != binding.create_path()? {
        return Err(TransformError::shape(
            "video.create.path",
            "path differs from selected Veo model/API prefix",
        ));
    }
    let input = input.into_declared();
    bound_value(&input, limits)?;
    bound_value(&binding, limits)?;
    let resources = input_resources(access, source_resource_scope, &input, limits).await?;
    let prepared =
        video::native_to_gemini_request(input.clone(), &binding.model, defaults, &resources)?;
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
        original: VideoOriginalRequest::Native(input),
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
    let response = match response {
        JsonInvocation::Success(response) => response,
        JsonInvocation::Rejected(r) => return Ok(JsonInvocation::Rejected(r)),
    };
    operation(&response.body, &state.binding)?;
    state.operation = Some(response.body.clone());
    let version = save(store, state_scope, &state, Some(version), limits).await?;
    progress.binding_saved = true;
    finish(
        store,
        state_scope,
        response,
        &mut state,
        version,
        facts,
        progress,
        limits,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn finish<
    S: StateStore,
    F: FnOnce(&g::VideoOperation) -> Result<NativeVideoResponseFacts, TransformError>,
>(
    store: &S,
    scope: &S::Scope,
    response: WireResponse<g::VideoOperation>,
    state: &mut VideoJobState,
    version: Version,
    facts: F,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::NativeVideo>>, TransformError> {
    let facts = facts(&response.body)?;
    bound_value(&facts, limits)?;
    let VideoOriginalRequest::Native(original) = &state.original else {
        return Err(TransformError::shape(
            "video.source",
            "native source job required",
        ));
    };
    if facts.client_id != state.binding.client_id
        || original.model.as_ref().is_some_and(|m| m != &facts.model)
    {
        return Err(TransformError::shape(
            "native.facts.identity",
            "native response facts differ from original client identity/model",
        ));
    }
    let expected = state
        .effective
        .parameters
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("native.effective_parameters"))?;
    let seconds = match facts.seconds {
        o::NativeVideoSeconds::Four => 4,
        o::NativeVideoSeconds::Eight => 8,
        o::NativeVideoSeconds::Twelve => 12,
    };
    let aspect = match facts.size {
        o::NativeVideoSize::Portrait720 => "9:16",
        o::NativeVideoSize::Landscape720 => "16:9",
        _ => {
            return Err(TransformError::shape(
                "native.facts.size",
                "result differs from exact Veo dimension mapping",
            ));
        }
    };
    if expected.duration_seconds != Some(seconds)
        || expected.aspect_ratio.as_deref() != Some(aspect)
        || expected.resolution.as_deref() != Some("720p")
        || facts.prompt.as_ref().is_some_and(|p| p != &original.prompt)
    {
        return Err(TransformError::shape(
            "native.facts.request",
            "native output facts contradict the effective request",
        ));
    }
    if state.native_facts.as_ref().is_some_and(|old| {
        old.created_at != facts.created_at
            || old.model != facts.model
            || old.seconds != facts.seconds
            || old.size != facts.size
            || old.progress > facts.progress
    }) {
        return Err(TransformError::shape(
            "native.facts.lifecycle",
            "stable native metadata changed or progress decreased",
        ));
    }
    let converted = video::gemini_operation_to_native(response.body, &facts)?;
    bound_value(&converted.value.body, limits)?;
    state.native_facts = Some(facts);
    save(store, scope, state, Some(version), limits).await?;
    progress.binding_saved = true;
    Ok(JsonInvocation::Success(WireResponse {
        status: response.status,
        headers: response.headers,
        body: Converted {
            value: converted.value.body,
            report: converted.report,
        },
    }))
}

#[allow(clippy::too_many_arguments)]
pub async fn native_to_gemini_query_composed<
    U: Upstream,
    S: StateStore,
    F: FnOnce(&g::VideoOperation) -> Result<NativeVideoResponseFacts, TransformError>,
>(
    upstream: &U,
    target: &U::Target,
    store: &S,
    state_scope: &S::Scope,
    template: WireRequest<()>,
    binding: VideoBinding,
    facts: F,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::NativeVideo>>, TransformError> {
    super::composed::fresh(progress)?;

    direct::check_template(&template, http::Method::GET)?;
    let (mut state, version) = load(store, state_scope, &binding, limits).await?;
    if !matches!(state.original, VideoOriginalRequest::Native(_)) {
        return Err(TransformError::shape(
            "video.source",
            "native source job required",
        ));
    }
    let old = state
        .operation
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("native.uncertain_operation"))?;
    let name = operation(old, &binding)?.to_owned();
    if template.path != binding.query_path(&name)? {
        return Err(TransformError::shape(
            "native.query.path",
            "path differs from stored operation",
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
    let response = match response {
        JsonInvocation::Success(response) => response,
        JsonInvocation::Rejected(r) => return Ok(JsonInvocation::Rejected(r)),
    };
    if operation(&response.body, &binding)? != name
        || old.done == Some(true)
            && (old.done != response.body.done
                || old.response != response.body.response
                || old.error != response.body.error)
    {
        return Err(TransformError::invalid_result(
            "native.query.lifecycle",
            "operation identity or terminal result changed",
        ));
    }
    state.operation = Some(response.body.clone());
    let version = save(store, state_scope, &state, Some(version), limits).await?;
    progress.binding_saved = true;
    finish(
        store,
        state_scope,
        response,
        &mut state,
        version,
        facts,
        progress,
        limits,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn recover_native_result<
    S: StateStore,
    F: FnOnce(&g::VideoOperation) -> Result<NativeVideoResponseFacts, TransformError>,
>(
    store: &S,
    scope: &S::Scope,
    binding: VideoBinding,
    facts: F,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::NativeVideo>>, TransformError> {
    if progress.binding.as_ref() != Some(&binding) || !progress.send_started {
        return Err(TransformError::shape(
            "native.recovery",
            "progress belongs to a different invocation",
        ));
    }
    let native = progress
        .native_response
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("native.recovery.response"))?;
    let response = WireResponse {
        status: native.status,
        headers: native.headers.clone(),
        body: native.body.clone(),
    };
    operation(&response.body, &binding)?;
    let (mut state, version) = load(store, scope, &binding, limits).await?;
    if state
        .operation
        .as_ref()
        .is_some_and(|old| old != &response.body)
    {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "native.recovery",
            "stored operation differs from retained source",
        ));
    }
    state.operation = Some(response.body.clone());
    let version = save(store, scope, &state, Some(version), limits).await?;
    finish(
        store, scope, response, &mut state, version, facts, progress, limits,
    )
    .await
}

/// Reads the actual completed video's bytes for the native `/content` route.
/// The selected target resource scope supplies authorization for private URIs.
pub async fn native_content<R: ResourceAccess, S: StateStore>(
    access: &R,
    target_resource_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    binding: VideoBinding,
    limits: VideoLimits,
) -> Result<ResourceRead, TransformError> {
    let (state, _) = load(store, state_scope, &binding, limits).await?;
    if !matches!(state.original, VideoOriginalRequest::Native(_)) {
        return Err(TransformError::shape(
            "video.source",
            "native source job required",
        ));
    }
    let facts = state
        .native_facts
        .ok_or_else(|| TransformError::missing_metadata("native.result_facts"))?;
    let expires_at = facts
        .expires_at
        .map(|seconds| {
            std::time::UNIX_EPOCH
                .checked_add(std::time::Duration::from_secs(seconds as u64))
                .ok_or_else(|| TransformError::shape("native.expires_at", "timestamp overflow"))
        })
        .transpose()?;
    if expires_at.is_some_and(|v| v <= std::time::SystemTime::now()) {
        return Err(TransformError::new(
            TransformErrorKind::MissingMetadata,
            "native.content",
            "saved video result has expired",
        ));
    }
    let result = video::gemini_operation_to_native(
        state
            .operation
            .ok_or_else(|| TransformError::missing_metadata("native.operation"))?,
        &facts,
    )?;
    if result.value.body.status != o::NativeVideoStatus::Completed {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "native.content",
            "video is not completed",
        ));
    }
    let video = result
        .value
        .videos
        .into_iter()
        .next()
        .ok_or_else(|| TransformError::missing_metadata("native.content"))?;
    let (bytes, mime) = if let Some(uri) = video.uri {
        read(
            access,
            target_resource_scope,
            &ResourceReference::Url(uri),
            false,
            limits,
            limits.codec.max_body_bytes,
        )
        .await?
    } else {
        let mime = video
            .encoding
            .ok_or_else(|| TransformError::missing_metadata("native.content.mime"))?;
        let bytes = decoded(
            video
                .encoded_video
                .as_deref()
                .ok_or_else(|| TransformError::missing_metadata("native.content.bytes"))?,
            &mime,
            false,
            limits.codec.max_body_bytes,
        )?;
        (bytes, mime)
    };
    Ok(ResourceRead {
        metadata: ResourceMetadata {
            mime: Some(mime),
            length: Some(bytes.len() as u64),
            filename: Some("video.mp4".into()),
            expires_at,
        },
        body: HttpBody::Bytes(bytes),
    })
}
