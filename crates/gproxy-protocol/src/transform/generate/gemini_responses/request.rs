use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{DeclaredFields, gemini as g, openai::responses as r},
};
pub fn gemini_to_responses_request(
    input: g::GenerateContentRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentRequestBody>, TransformError> {
    if policy.dialect != crate::Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "Responses target required",
        ));
    }
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let input = input.into_declared();
    let mut ids = flow.clone();
    let mut report = Report::default();
    let mut out = r::GenerateContentRequestBody::builder().build();
    super::config::to_responses(&input, &mut out, &mut report)?;
    out.model = Some(model.clone());
    let validated = input
        .tool_config
        .as_ref()
        .and_then(|v| v.function_calling_config.as_ref())
        .and_then(|v| v.mode.as_ref())
        == Some(&g::FunctionCallingMode::Validated);
    out.tools = input
        .tools
        .map(|tools| super::tools::to_responses(tools, validated, &mut report))
        .transpose()?;
    out.tool_choice = input
        .tool_config
        .map(super::tools::choice_to_responses)
        .transpose()?
        .flatten();
    if let Some(system) = input.system_instruction {
        let mut text = String::new();
        for part in system.parts.unwrap_or_default() {
            super::content::validate(&part)?;
            if part.inline_data.is_some()
                || part.file_data.is_some()
                || part.function_call.is_some()
                || part.function_response.is_some()
                || part.thought == Some(true)
            {
                return Err(TransformError::unsupported(
                    "system",
                    "Responses instructions require text",
                ));
            }
            text.push_str(part.text.as_deref().unwrap_or(""));
        }
        out.instructions = Some(Some(text));
    }
    out.input = Some(r::Input::Items(super::content::to_responses(
        input.contents,
        &mut ids,
        policy,
        &mut report,
    )?));
    super::request_images::to_responses(
        input.generation_config.as_ref(),
        &mut out.tools,
        &mut out.tool_choice,
    )?;
    crate::transform::instructions::responses(
        &mut out.input,
        &mut out.instructions,
        &model,
        &mut report,
    );
    *flow = ids;
    Ok(Converted { value: out, report })
}
pub fn responses_to_gemini_request(
    input: r::GenerateContentRequestBody,
    target_model: impl Into<String>,
    mut context: super::identity::GeminiReplayContext,
) -> Result<Converted<g::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let mut input = input.into_declared();
    let mut out = g::GenerateContentRequestBody::builder(Vec::new()).build();
    let mut report = Report::default();
    super::config::to_gemini(&input, &mut out, &mut report)?;
    super::request_images::to_gemini(
        &mut input.tools,
        &mut input.tool_choice,
        &mut out.generation_config,
        &mut report,
    )?;
    let strict = if let Some(tools) = input.tools {
        let (tools, strict) = super::tools::to_gemini(tools)?;
        out.tools = Some(tools);
        strict
    } else {
        false
    };
    out.tool_config = super::tools::choice_to_gemini(input.tool_choice, strict)?;
    let (contents, mut system) =
        super::history::to_gemini(input.input, &model, &mut context, &mut report)?;
    out.contents = contents;
    if let Some(Some(text)) = input.instructions {
        system.insert(0, g::Part::builder().text(text).build());
    }
    if !system.is_empty() {
        out.system_instruction = Some(g::Content::builder().parts(system).build());
    }
    Ok(Converted { value: out, report })
}
