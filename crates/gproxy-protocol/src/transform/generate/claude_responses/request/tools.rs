use crate::{
    transform::{Report, TransformError},
    wire::{
        claude::{count_tokens as cc, tools as c},
        openai::responses::{input as i, tools as r},
    },
};
pub(super) fn to_responses(
    input: Vec<c::ToolUnion>,
    report: &mut Report,
) -> Result<Vec<r::Tool>, TransformError> {
    input
        .into_iter()
        .map(|tool| {
            let c::ToolUnion::Custom(tool) = tool else {
                return Err(TransformError::unsupported(
                    "tools",
                    "Claude hosted tool requires Responses host binding",
                ));
            };
            if tool
                .allowed_callers
                .as_ref()
                .is_some_and(|v| v.iter().any(|v| !matches!(v, c::AllowedCaller::Direct)))
            {
                return Err(TransformError::unsupported(
                    "tools.allowed_callers",
                    "versioned Claude execution caller cannot be broadened",
                ));
            }
            let schema = super::schema::to_chat(&tool.input_schema)?;
            let mut target = r::FunctionTool::builder(tool.name, Some(schema), tool.strict).build();
            target.description = Some(tool.description);
            target.defer_loading = tool.defer_loading;
            if tool.cache_control.is_some()
                || tool.eager_input_streaming.is_some()
                || tool.input_examples.is_some()
            {
                report.omitted(
                    "tools.advisory",
                    "target lacks per-tool cache/input examples/eager streaming controls",
                );
            }
            Ok(r::Tool::Function(target))
        })
        .collect()
}
pub(super) fn to_claude(input: Vec<r::Tool>) -> Result<Vec<c::ToolUnion>, TransformError> {
    input
        .into_iter()
        .map(|tool| {
            let r::Tool::Function(tool) = tool else {
                return Err(TransformError::unsupported(
                    "tools",
                    "Responses hosted/custom tools need explicit Claude binding",
                ));
            };
            if tool.allowed_callers.flatten().is_some_and(|v| {
                v.iter()
                    .any(|v| matches!(v, r::AllowedCaller::Programmatic))
            }) || tool.output_schema.is_some()
            {
                return Err(TransformError::unsupported(
                    "tools",
                    "Claude function lacks matching caller/output schema",
                ));
            }
            let schema = match tool.parameters {
                Some(schema) => super::schema::from_chat(&schema)?,
                None => c::JsonSchema::builder(c::JsonSchemaType::Object).build(),
            };
            let mut target = c::Tool::builder(schema, tool.name).build();
            target.description = tool.description.flatten();
            target.strict = tool.strict;
            target.defer_loading = tool.defer_loading;
            Ok(c::ToolUnion::Custom(target))
        })
        .collect()
}
pub(super) fn choice_to_responses(choice: cc::ToolChoice) -> (i::ToolChoice, Option<bool>) {
    match choice {
        cc::ToolChoice::Auto(v) => (
            i::ToolChoice::Mode(i::ToolChoiceMode::Auto),
            v.disable_parallel_tool_use.map(|v| !v),
        ),
        cc::ToolChoice::Any(v) => (
            i::ToolChoice::Mode(i::ToolChoiceMode::Required),
            v.disable_parallel_tool_use.map(|v| !v),
        ),
        cc::ToolChoice::None(_) => (i::ToolChoice::Mode(i::ToolChoiceMode::None), None),
        cc::ToolChoice::Tool(v) => (
            i::ToolChoice::Function(
                i::ToolChoiceFunction::builder(
                    i::ToolChoiceFunctionType::ToolChoiceFunction,
                    v.name,
                )
                .build(),
            ),
            v.disable_parallel_tool_use.map(|v| !v),
        ),
    }
}
pub(super) fn choice_to_claude(
    choice: i::ToolChoice,
    parallel: Option<bool>,
) -> Result<cc::ToolChoice, TransformError> {
    let disable = parallel.map(|v| !v);
    Ok(match choice {
        i::ToolChoice::Mode(i::ToolChoiceMode::Auto) => {
            let mut v = cc::ToolChoiceAuto::builder().build();
            v.disable_parallel_tool_use = disable;
            cc::ToolChoice::Auto(v)
        }
        i::ToolChoice::Mode(i::ToolChoiceMode::Required) => {
            let mut v = cc::ToolChoiceAny::builder().build();
            v.disable_parallel_tool_use = disable;
            cc::ToolChoice::Any(v)
        }
        i::ToolChoice::Mode(i::ToolChoiceMode::None) => {
            cc::ToolChoice::None(cc::ToolChoiceNone::builder().build())
        }
        i::ToolChoice::Function(v) => {
            let mut out = cc::ToolChoiceTool::builder(v.name).build();
            out.disable_parallel_tool_use = disable;
            cc::ToolChoice::Tool(out)
        }
        i::ToolChoice::Allowed(_)
        | i::ToolChoice::Hosted(_)
        | i::ToolChoice::Mcp(_)
        | i::ToolChoice::Custom(_)
        | i::ToolChoice::Programmatic(_)
        | i::ToolChoice::ApplyPatch(_)
        | i::ToolChoice::Shell(_) => {
            return Err(TransformError::unsupported(
                "tool_choice",
                "Responses subset/hosted/custom choice requires tool adaptation",
            ));
        }
    })
}
