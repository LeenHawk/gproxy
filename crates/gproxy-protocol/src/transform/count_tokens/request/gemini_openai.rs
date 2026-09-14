use crate::{
    transform::{
        Converted, Report, TransformError,
        generate::gemini_responses as pair,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{DeclaredFields, gemini as g, openai::count_tokens as o},
};
pub fn gemini_to_openai(
    input: g::CountTokensRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<o::CountTokensRequestBody>, TransformError> {
    super::policy(policy, crate::Dialect::OpenAi)?;
    let model = super::model(target_model)?;
    let input = super::gemini_input(input.into_declared())?;
    let mut ids = flow.clone();
    let mut report = Report::default();
    let mut out = o::CountTokensRequestBody::builder().build();
    out.model = Some(Some(model));
    super::controls::gemini_to_openai(&input, &mut out, &mut report)?;
    let validated = input
        .tool_config
        .as_ref()
        .and_then(|v| v.function_calling_config.as_ref())
        .and_then(|v| v.mode.as_ref())
        == Some(&g::FunctionCallingMode::Validated);
    out.tools = input
        .tools
        .map(|tools| pair::tools::to_responses(tools, validated, &mut report))
        .transpose()?
        .map(Some);
    out.tool_choice = input
        .tool_config
        .map(pair::tools::choice_to_responses)
        .transpose()?
        .flatten()
        .map(Some);
    let mut image_tools = out.tools.take().flatten();
    let mut image_choice = out.tool_choice.take().flatten();
    pair::request_images::to_responses(
        input.generation_config.as_ref(),
        &mut image_tools,
        &mut image_choice,
    )?;
    out.tools = image_tools.map(Some);
    out.tool_choice = image_choice.map(Some);
    if let Some(system) = input.system_instruction {
        let mut text = String::new();
        for part in system.parts.unwrap_or_default() {
            pair::content::validate(&part)?;
            if part.inline_data.is_some()
                || part.file_data.is_some()
                || part.function_call.is_some()
                || part.function_response.is_some()
                || part.thought == Some(true)
            {
                return Err(TransformError::unsupported(
                    "system_instruction",
                    "count instructions require plain text",
                ));
            }
            text.push_str(part.text.as_deref().unwrap_or(""));
        }
        out.instructions = Some(Some(text));
    }
    out.input = Some(Some(o::Input::Items(pair::content::to_responses(
        input.contents,
        &mut ids,
        policy,
        &mut report,
    )?)));
    *flow = ids;
    Ok(Converted { value: out, report })
}
pub fn openai_to_gemini(
    input: o::CountTokensRequestBody,
    target_model: impl Into<String>,
    mut context: pair::GeminiReplayContext,
) -> Result<Converted<g::CountTokensRequestBody>, TransformError> {
    let model = super::model(target_model)?;
    let input = input.into_declared();
    super::openai_state(&input)?;
    let mut report = Report::default();
    let mut config = super::controls::openai_to_gemini(&input, &mut report)?;
    let mut image_tools = input.tools.flatten();
    let mut image_choice = input.tool_choice.flatten();
    pair::request_images::to_gemini(
        &mut image_tools,
        &mut image_choice,
        &mut config,
        &mut report,
    )?;
    let (tools, strict) = match image_tools {
        Some(tools) => {
            let (tools, strict) = pair::tools::to_gemini(tools)?;
            (Some(tools), strict)
        }
        None => (None, false),
    };
    let choice = pair::tools::choice_to_gemini(image_choice, strict)?;
    let (contents, mut system) =
        pair::history::to_gemini(input.input.flatten(), &model, &mut context, &mut report)?;
    if let Some(Some(text)) = input.instructions {
        system.insert(0, g::Part::builder().text(text).build());
    }
    let system = (!system.is_empty()).then(|| g::Content::builder().parts(system).build());
    Ok(Converted {
        value: super::gemini_target(model, contents, system, tools, choice, config),
        report,
    })
}
