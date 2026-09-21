use super::*;

pub(super) fn facts(
    resources: &BTreeMap<String, ResolvedVideoResource>,
    limits: VideoLimits,
) -> Result<(), TransformError> {
    if resources.len() > limits.max_resource_facts {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "video.resources",
            "resource fact bound exceeded",
        ));
    }
    super::resources::bound_value(
        &resources
            .iter()
            .map(|(key, value)| {
                (
                    key,
                    &value.reference,
                    &value.url,
                    &value.bytes_base64_encoded,
                    &value.mime_type,
                )
            })
            .collect::<Vec<_>>(),
        limits,
    )?;
    Ok(())
}

pub(super) fn check_template(
    request: &WireRequest<()>,
    method: http::Method,
) -> Result<(), TransformError> {
    if request.method != method
        || request
            .query
            .as_ref()
            .is_some_and(|query| !query.is_empty())
        || !request.path.starts_with('/')
        || request.path.starts_with("//")
        || request.path.contains(['?', '#', '\\', '\r', '\n'])
    {
        return Err(TransformError::shape(
            "video.template",
            "origin-relative method-matched template required",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn openai_to_gemini_create<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: o::CreateVideoRequestBody,
    target_model: &str,
    resources: BTreeMap<String, ResolvedVideoResource>,
    context: OpenAiVideoResponseContext,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    facts(&context.resources, limits)?;
    facts(&resources, limits)?;
    check_template(&template, http::Method::POST)?;
    let input = input.into_declared();
    super::resources::bound_value(&input, limits)?;
    let prepared = video::openai_to_gemini_request(input, target_model, &resources)?;
    let request = WireRequest {
        method: template.method,
        path: template.path,
        query: template.query,
        headers: template.headers,
        body: prepared.value.body,
    };
    match invoke_json::<_, _, g::VideoOperation>(upstream, target, request, limits.codec).await? {
        JsonInvocation::Rejected(response) => Ok(JsonInvocation::Rejected(response)),
        JsonInvocation::Success(response) => Ok(JsonInvocation::Success(crate::WireResponse {
            status: response.status,
            headers: response.headers,
            body: video::gemini_operation_to_openai_response(response.body, &context)?,
        })),
    }
}

pub async fn openai_to_gemini_query<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    context: OpenAiVideoResponseContext,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<o::VideoGenerationResponseBody>>, TransformError> {
    facts(&context.resources, limits)?;
    check_template(&template, http::Method::GET)?;
    let response =
        invoke_empty::<_, g::VideoOperation>(upstream, target, template, limits.codec).await?;
    match response {
        JsonInvocation::Success(response) => Ok(JsonInvocation::Success(crate::WireResponse {
            status: response.status,
            headers: response.headers,
            body: video::gemini_operation_to_openai_response(response.body, &context)?,
        })),
        JsonInvocation::Rejected(response) => Ok(JsonInvocation::Rejected(response)),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn gemini_to_openai_create<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: g::PredictLongRunningRequestBody,
    target_model: &str,
    resources: BTreeMap<String, ResolvedVideoResource>,
    context: GeminiOperationContext,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    facts(&resources, limits)?;
    check_template(&template, http::Method::POST)?;
    let input = input.into_declared();
    super::resources::bound_value(&input, limits)?;
    let prepared = video::gemini_to_openai_request(input, target_model, &resources)?;
    let request = WireRequest {
        method: template.method,
        path: template.path,
        query: template.query,
        headers: template.headers,
        body: prepared.value.body,
    };
    match invoke_json::<_, _, o::VideoGenerationResponseBody>(
        upstream,
        target,
        request,
        limits.codec,
    )
    .await?
    {
        JsonInvocation::Rejected(response) => Ok(JsonInvocation::Rejected(response)),
        JsonInvocation::Success(response) => Ok(JsonInvocation::Success(crate::WireResponse {
            status: response.status,
            headers: response.headers,
            body: video::openai_response_to_gemini_operation(response.body, &context)?,
        })),
    }
}

pub async fn gemini_to_openai_query<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    context: GeminiOperationContext,
    limits: VideoLimits,
) -> Result<JsonInvocation<Converted<g::VideoOperation>>, TransformError> {
    check_template(&template, http::Method::GET)?;
    let response =
        invoke_empty::<_, o::VideoGenerationResponseBody>(upstream, target, template, limits.codec)
            .await?;
    match response {
        JsonInvocation::Success(response) => Ok(JsonInvocation::Success(crate::WireResponse {
            status: response.status,
            headers: response.headers,
            body: video::openai_response_to_gemini_operation(response.body, &context)?,
        })),
        JsonInvocation::Rejected(response) => Ok(JsonInvocation::Rejected(response)),
    }
}

/// Native Sora-style create/retrieve preserve their own ID, seconds and size
/// contract; they do not pass through the OpenRouter/Veo mapper.
pub async fn native_create<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<o::NativeCreateVideoRequestBody>,
    limits: VideoLimits,
) -> Result<JsonInvocation<o::NativeVideo>, TransformError> {
    if limits.max_resource_facts == 0 {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "video.resources",
            "positive resource fact bound required",
        ));
    }
    if request.method != http::Method::POST
        || request
            .query
            .as_ref()
            .is_some_and(|query| !query.is_empty())
        || !request.path.starts_with('/')
        || request.path.starts_with("//")
        || request.path.contains(['?', '#', '\\', '\r', '\n'])
    {
        return Err(TransformError::shape(
            "video.template",
            "origin-relative POST template required",
        ));
    }
    invoke_json(upstream, target, request, limits.codec).await
}

pub async fn native_retrieve<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: WireRequest<()>,
    limits: VideoLimits,
) -> Result<JsonInvocation<o::NativeVideo>, TransformError> {
    check_template(&request, http::Method::GET)?;
    invoke_empty(upstream, target, request, limits.codec).await
}
