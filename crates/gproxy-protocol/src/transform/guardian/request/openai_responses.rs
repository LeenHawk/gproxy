use super::*;

pub fn prepare_openai_responses(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_openai_responses_with_limits(input, context, default_history_limits())
}

pub fn prepare_openai_responses_with_limits(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_openai_responses_plan(input, context, limits, true)
}

pub(super) fn prepare_openai_responses_plan(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
    attach: bool,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    validate(&context)?;
    let input = input.into_declared();
    validate_input(&input)?;
    if context.operation == GuardianOperation::Classify
        && input.text.as_ref().is_some_and(|v| v.format.is_some())
    {
        return Err(TransformError::unsupported(
            "guardian.text.format",
            "classification requires the bare high/low label",
        ));
    }
    bound_source(&input, limits)?;
    let payload = task_payload(&input, context.operation, limits)?;
    let mut body = r::GenerateContentRequestBody::builder()
        .model(context.target_model.clone())
        .instructions(instructions(&input, context.operation))
        .input(r::Input::Text(payload))
        .max_output_tokens(Some(context.max_tokens))
        .stream(Some(false))
        .build();
    body.service_tier = input
        .service_tier
        .as_deref()
        .map(|v| {
            Ok(Some(match v {
                "auto" => r::ServiceTier::Auto,
                "default" => r::ServiceTier::Default,
                "flex" => r::ServiceTier::Flex,
                "scale" => r::ServiceTier::Scale,
                "priority" => r::ServiceTier::Priority,
                "fast" => r::ServiceTier::Fast,
                _ => {
                    return Err(TransformError::unsupported(
                        "guardian.service_tier",
                        "unknown tier",
                    ));
                }
            }))
        })
        .transpose()?;
    body.store = Some(Some(input.store));
    body.prompt_cache_key = input.prompt_cache_key.clone().map(Some);
    body.reasoning = effort(&input)
        .map(|value| {
            responses_effort(value)
                .map(|effort| Some(r::ReasoningConfig::builder().effort(Some(effort)).build()))
        })
        .transpose()?;
    if let Some(source) = &input.reasoning {
        let config = body
            .reasoning
            .get_or_insert_with(|| Some(r::ReasoningConfig::builder().build()))
            .get_or_insert_with(|| r::ReasoningConfig::builder().build());
        config.summary = source.summary.as_ref().map(|v| match v {
            source::ReasoningSummary::None => None,
            source::ReasoningSummary::Auto => Some(r::ReasoningSummary::Auto),
            source::ReasoningSummary::Concise => Some(r::ReasoningSummary::Concise),
            source::ReasoningSummary::Detailed => Some(r::ReasoningSummary::Detailed),
        });
        config.context = source.context.as_ref().map(|v| {
            Some(match v {
                source::ReasoningContext::Auto => r::ReasoningContext::Auto,
                source::ReasoningContext::CurrentTurn => r::ReasoningContext::CurrentTurn,
                source::ReasoningContext::AllTurns => r::ReasoningContext::AllTurns,
            })
        });
    }
    if context.operation == GuardianOperation::Review {
        let (name, output_schema, strict) = review_schema(&input)?;
        body.text = Some(
            r::TextConfig::builder()
                .format(r::TextFormat::JsonSchema(
                    r::TextFormatJsonSchema::builder(
                        name,
                        output_schema.as_object().cloned().ok_or_else(|| {
                            TransformError::shape(
                                "guardian.text.format.schema",
                                "JSON schema must be an object",
                            )
                        })?,
                    )
                    .strict(Some(strict))
                    .build(),
                ))
                .build(),
        );
    }
    if let Some(v) = input.text.as_ref().and_then(|v| v.verbosity.as_ref()) {
        body.text
            .get_or_insert_with(|| r::TextConfig::builder().build())
            .verbosity = Some(Some(match v {
            source::ClientVerbosity::Low => r::TextVerbosity::Low,
            source::ClientVerbosity::Medium => r::TextVerbosity::Medium,
            source::ClientVerbosity::High => r::TextVerbosity::High,
        }));
    }
    body.include = Some(Some(
        input
            .include
            .iter()
            .map(|v| {
                Ok(match v.as_str() {
                    "web_search_call.action.sources" => r::ResponseIncludable::WebSearchSources,
                    "code_interpreter_call.outputs" => {
                        r::ResponseIncludable::CodeInterpreterOutputs
                    }
                    "computer_call_output.output.image_url" => {
                        r::ResponseIncludable::ComputerImageUrl
                    }
                    "file_search_call.results" => r::ResponseIncludable::FileSearchResults,
                    "message.input_image.image_url" => r::ResponseIncludable::InputImageUrl,
                    "message.output_text.logprobs" => r::ResponseIncludable::OutputLogprobs,
                    "reasoning.encrypted_content" => {
                        r::ResponseIncludable::ReasoningEncryptedContent
                    }
                    _ => {
                        return Err(TransformError::unsupported(
                            "guardian.include",
                            "unknown native include selector",
                        ));
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    ));
    let mut target = GuardianDialectRequest::OpenAiResponses(request("/v1/responses", body));
    let media = media(&input)?;
    validate_media(&target, &media)?;
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
