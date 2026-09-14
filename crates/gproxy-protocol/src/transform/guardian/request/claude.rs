use super::*;

pub fn prepare_claude(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_claude_with_limits(input, context, default_history_limits())
}

pub fn prepare_claude_with_limits(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_claude_plan(input, context, limits, true)
}

pub(super) fn prepare_claude_plan(
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
    let mut body = cg::GenerateContentRequestBody::builder(
        context.max_tokens,
        vec![cc::Message::builder(cc::Role::User, cc::MessageContent::Text(payload)).build()],
        context.target_model.clone(),
    )
    .build();
    body.system = Some(ct::SystemPrompt::Text(instructions(
        &input,
        context.operation,
    )));
    body.stream = Some(false);
    body.service_tier = input
        .service_tier
        .as_deref()
        .map(|value| {
            Ok(match value {
                "auto" => cg::ServiceTier::Auto,
                "default" => cg::ServiceTier::StandardOnly,
                _ => {
                    return Err(TransformError::unsupported(
                        "guardian.service_tier",
                        "Claude lacks requested service tier",
                    ));
                }
            })
        })
        .transpose()?;
    if input.text.as_ref().is_some_and(|v| v.verbosity.is_some()) {
        return Err(TransformError::unsupported(
            "guardian.text.verbosity",
            "selected target lacks requested verbosity control",
        ));
    }
    non_responses_controls(&input)?;
    if input.store {
        return Err(TransformError::unsupported(
            "guardian.store",
            "Claude requires host stored-response state",
        ));
    }
    match effort(&input) {
        None => {}
        Some("none") => {
            body.thinking = Some(ct::ThinkingConfig::Disabled(
                ct::ThinkingDisabled::builder().build(),
            ))
        }
        Some(level) => {
            let level = match level {
                "low" => ct::Effort::Low,
                "medium" => ct::Effort::Medium,
                "high" => ct::Effort::High,
                "xhigh" => ct::Effort::Xhigh,
                "max" => ct::Effort::Max,
                _ => {
                    return Err(TransformError::unsupported(
                        "guardian.reasoning.effort",
                        "Claude lacks requested effort",
                    ));
                }
            };
            body.thinking = Some(ct::ThinkingConfig::Adaptive(
                ct::ThinkingAdaptive::builder().build(),
            ));
            body.output_config = Some(ct::OutputConfig::builder().effort(level).build());
        }
    }
    if context.operation == GuardianOperation::Review {
        let (_, output_schema, _) = review_schema(&input)?;
        body.output_format = Some(
            ct::JsonOutputFormat::builder(ct::JsonOutputFormatType::JsonSchema, output_schema)
                .build(),
        );
    }
    let mut target = GuardianDialectRequest::Claude(request("/v1/messages", body));
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
