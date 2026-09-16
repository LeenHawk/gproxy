use super::*;

pub fn prepare_gemini(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_gemini_with_limits(input, context, default_history_limits())
}

pub fn prepare_gemini_with_limits(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    prepare_gemini_plan(input, context, limits, true)
}

pub(super) fn prepare_gemini_plan(
    input: source::GuardianRequestBody,
    context: GuardianRequestContext,
    limits: CodecLimits,
    attach: bool,
) -> Result<Converted<GuardianPreparedRequest>, TransformError> {
    let input = input.into_declared();

    bound_source(&input, limits)?;
    let payload = task_payload(&input, context.operation, limits)?;
    let model = context
        .target_model
        .strip_prefix("models/")
        .unwrap_or(&context.target_model);
    if model.is_empty()
        || model.contains(['/', '?', '#', '\\'])
        || model.chars().any(char::is_control)
    {
        return Err(TransformError::shape(
            "guardian.target_model",
            "one Gemini model path segment required",
        ));
    }
    let model = encode_model_segment(model);
    let mut generation = g::GenerationConfig::builder()
        .max_output_tokens(context.max_tokens)
        .build();
    let service_tier = input
        .service_tier
        .as_deref()
        .map(|value| {
            Ok(match value {
                "auto" => g::ServiceTier::Unspecified,
                "default" => g::ServiceTier::Standard,
                "flex" => g::ServiceTier::Flex,
                "priority" => g::ServiceTier::Priority,
                _ => {
                    return Err(TransformError::unsupported(
                        "guardian.service_tier",
                        "Gemini lacks requested service tier",
                    ));
                }
            })
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();

    if let Some(level) = effort(&input) {
        let mut thinking = g::ThinkingConfig::builder().build();
        match level {
            "none" => thinking.thinking_budget = Some(0),
            "minimal" => thinking.thinking_level = Some(g::ThinkingLevel::Minimal),
            "low" => thinking.thinking_level = Some(g::ThinkingLevel::Low),
            "medium" => thinking.thinking_level = Some(g::ThinkingLevel::Medium),
            "high" => thinking.thinking_level = Some(g::ThinkingLevel::High),
            _ => {}
        }
        generation.thinking_config = Some(thinking);
    }
    if context.operation == GuardianOperation::Review {
        let (_, output_schema, _) = review_schema(&input)?;
        generation.response_mime_type = Some("application/json".into());
        generation.response_json_schema = Some(output_schema);
    }
    let mut body = g::GenerateContentRequestBody::builder(vec![
        g::Content::builder()
            .role("user")
            .parts(vec![g::Part::builder().text(payload).build()])
            .build(),
    ])
    .system_instruction(
        g::Content::builder()
            .role("system")
            .parts(vec![
                g::Part::builder()
                    .text(instructions(&input, context.operation))
                    .build(),
            ])
            .build(),
    )
    .generation_config(generation)
    .store(input.store)
    .build();
    body.service_tier = service_tier;
    let mut target = GuardianDialectRequest::Gemini(request(
        format!("/v1beta/models/{model}:generateContent"),
        body,
    ));
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

fn encode_model_segment(value: &str) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            output.push(char::from(byte));
        } else {
            write!(&mut output, "%{byte:02X}").expect("writing to String");
        }
    }
    output
}
