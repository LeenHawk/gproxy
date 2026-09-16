use super::*;

pub fn prepare_openai_chat(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_openai_chat_with_limits(input, context, default_history_limits())
}

pub fn prepare_openai_chat_with_limits(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_openai_chat_plan(input, context, limits, true)
}

pub(super) fn prepare_openai_chat_plan(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
    attach: bool,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    let input = input.into_declared();

    bound_source(&input, limits)?;
    let payload = task_payload(&input, context.operation, limits)?;
    let mut body = o::GenerateContentRequestBody::builder(
        vec![
            o::ChatMessage::System(
                o::SystemMessage::builder(
                    o::SystemRole::System,
                    o::TextContent::Text(instructions(&input, context.operation)),
                )
                .build(),
            ),
            o::ChatMessage::User(
                o::UserMessage::builder(o::UserRole::User, o::UserContent::Text(payload)).build(),
            ),
        ],
        context.target_model.clone(),
    )
    .build();
    body.max_completion_tokens = Some(Some(context.max_tokens));
    body.stream = Some(Some(false));

    body.service_tier = input
        .service_tier
        .as_deref()
        .map(|v| {
            Ok(Some(match v {
                "auto" => o::ServiceTier::Auto,
                "default" => o::ServiceTier::Default,
                "flex" => o::ServiceTier::Flex,
                "scale" => o::ServiceTier::Scale,
                "priority" => o::ServiceTier::Priority,
                "fast" => o::ServiceTier::Fast,
                _ => {
                    return Err(TransformError::unsupported(
                        "guardian.service_tier",
                        "unknown tier",
                    ));
                }
            }))
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    body.store = Some(Some(input.store));
    body.prompt_cache_key = input.prompt_cache_key.clone().map(Some);
    body.verbosity = input
        .text
        .as_ref()
        .and_then(|v| v.verbosity.as_ref())
        .map(|v| {
            Some(match v {
                source::ClientVerbosity::Low => o::Verbosity::Low,
                source::ClientVerbosity::Medium => o::Verbosity::Medium,
                source::ClientVerbosity::High => o::Verbosity::High,
            })
        });
    body.reasoning_effort = effort(&input).and_then(chat_effort).map(Some);
    if context.operation == GuardianOperation::Review {
        let (name, output_schema, strict) = review_schema(&input)?;
        body.response_format = Some(o::ResponseFormat::JsonSchema(
            o::JsonSchemaResponseFormat::builder(
                o::JsonSchemaResponseType::JsonSchema,
                o::JsonSchemaFormat::builder(name)
                    .schema(output_schema.as_object().cloned().ok_or_else(|| {
                        TransformError::shape(
                            "guardian.text.format.schema",
                            "JSON schema must be an object",
                        )
                    })?)
                    .strict(Some(strict))
                    .build(),
            )
            .build(),
        ));
    }
    let mut target = GuardianDialectRequest::OpenAiChat(request("/v1/chat/completions", body));
    let media = media(&input)?;

    if attach {
        attach_media(&mut target, &media)?;
    }
    preflight_request(&target, limits, limits.max_body_bytes)?;
    let report = report(&input, &target);
    Ok(Converted {
        value: GuardianPreparedRequest {
            operation: context.operation,
            request: target,
            source: input,
        },
        report,
    })
}
