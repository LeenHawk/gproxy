use crate::wire::claude::count_tokens as cc;

use crate::{
    Rest,
    transform::{Report, TransformError},
    wire::{claude::tools as ct, openai::chat},
};

pub(crate) fn claude_tools_to_openai(
    tools: Option<&Vec<ct::ToolUnion>>,
    report: &mut Report,
) -> Result<Option<Vec<chat::ChatTool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        let ct::ToolUnion::Custom(tool) = tool else {
            return Err(TransformError::unsupported(
                "tools",
                "non-custom Claude server tool needs a host capability",
            ));
        };
        if tool.allowed_callers.as_ref().is_some_and(|callers| {
            callers
                .iter()
                .any(|caller| !matches!(caller, ct::AllowedCaller::Direct))
        }) || tool.defer_loading == Some(true)
        {
            return Err(TransformError::unsupported(
                "tools.allowed_callers/defer_loading",
                "Chat function tools cannot preserve execution restrictions or deferred loading",
            ));
        }
        for (present, field) in [
            (tool.cache_control.is_some(), "cache_control"),
            (
                tool.eager_input_streaming.is_some(),
                "eager_input_streaming",
            ),
            (tool.input_examples.is_some(), "input_examples"),
        ] {
            if present {
                report.omitted(
                    format!("tools.{field}"),
                    "Chat function definitions have no equivalent advisory field",
                );
            }
        }
        let parameters = super::schema::to_chat(&tool.input_schema)?;
        output.push(chat::ChatTool::Function(chat::FunctionTool {
            type_: chat::FunctionToolType::Function,
            function: chat::FunctionDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: Some(parameters),
                strict: tool.strict.map(Some),
                rest: Rest::new(),
            },
            rest: Rest::new(),
        }));
    }
    if output.is_empty() {
        report.omitted("tools", "empty tool declaration omitted");
    }
    Ok(Some(output))
}

pub(crate) fn openai_tools_to_claude(
    tools: Option<&Vec<chat::ChatTool>>,
) -> Result<Option<Vec<ct::ToolUnion>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        let chat::ChatTool::Function(tool) = tool else {
            return Err(TransformError::unsupported(
                "tools",
                "OpenAI custom tools need a Claude host capability",
            ));
        };
        let input_schema = match tool.function.parameters.as_ref() {
            Some(parameters) => super::schema::from_chat(parameters)?,
            None => {
                let mut schema = ct::JsonSchema::builder(ct::JsonSchemaType::Object).build();
                schema.properties = Some(serde_json::json!({}));
                schema.additional_properties = Some(serde_json::Value::Bool(false));
                schema
            }
        };
        output.push(ct::ToolUnion::Custom(ct::Tool {
            input_schema,
            name: tool.function.name.clone(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            description: tool.function.description.clone(),
            eager_input_streaming: None,
            input_examples: None,
            strict: tool.function.strict.flatten(),
            type_: None,
            rest: Rest::new(),
        }));
    }
    Ok(Some(output))
}

pub(crate) fn claude_choice_to_openai(
    choice: Option<&cc::ToolChoice>,
) -> Result<Option<chat::ToolChoice>, TransformError> {
    let Some(choice) = choice else {
        return Ok(None);
    };
    Ok(Some(match choice {
        cc::ToolChoice::Auto(_) => chat::ToolChoice::Mode(chat::ToolChoiceMode::Auto),
        cc::ToolChoice::Any(_) => chat::ToolChoice::Mode(chat::ToolChoiceMode::Required),
        cc::ToolChoice::None(_) => chat::ToolChoice::Mode(chat::ToolChoiceMode::None),
        cc::ToolChoice::Tool(tool) => chat::ToolChoice::Function(chat::NamedFunctionChoice {
            type_: chat::NamedFunctionChoiceType::Function,
            function: chat::NamedFunction {
                name: tool.name.clone(),
                rest: Rest::new(),
            },
            rest: Rest::new(),
        }),
    }))
}

pub(crate) fn openai_choice_to_claude(
    choice: Option<&chat::ToolChoice>,
) -> Result<Option<cc::ToolChoice>, TransformError> {
    let Some(choice) = choice else {
        return Ok(None);
    };
    Ok(Some(match choice {
        chat::ToolChoice::Mode(chat::ToolChoiceMode::Auto) => {
            cc::ToolChoice::Auto(cc::ToolChoiceAuto {
                disable_parallel_tool_use: None,
                rest: Rest::new(),
            })
        }
        chat::ToolChoice::Mode(chat::ToolChoiceMode::Required) => {
            cc::ToolChoice::Any(cc::ToolChoiceAny {
                disable_parallel_tool_use: None,
                rest: Rest::new(),
            })
        }
        chat::ToolChoice::Mode(chat::ToolChoiceMode::None) => {
            cc::ToolChoice::None(cc::ToolChoiceNone { rest: Rest::new() })
        }
        chat::ToolChoice::Function(choice) => cc::ToolChoice::Tool(cc::ToolChoiceTool {
            name: choice.function.name.clone(),
            disable_parallel_tool_use: None,
            rest: Rest::new(),
        }),
        chat::ToolChoice::Custom(_) | chat::ToolChoice::Allowed(_) => {
            return Err(TransformError::unsupported(
                "tool_choice",
                "custom/allowed Chat choices have no Claude equivalent",
            ));
        }
    }))
}

pub(super) fn legacy_choice(choice: &chat::FunctionCallChoice) -> cc::ToolChoice {
    match choice {
        chat::FunctionCallChoice::Mode(chat::FunctionCallMode::Auto) => {
            cc::ToolChoice::Auto(cc::ToolChoiceAuto::builder().build())
        }
        chat::FunctionCallChoice::Mode(chat::FunctionCallMode::None) => {
            cc::ToolChoice::None(cc::ToolChoiceNone::builder().build())
        }
        chat::FunctionCallChoice::Name(choice) => {
            cc::ToolChoice::Tool(cc::ToolChoiceTool::builder(choice.name.clone()).build())
        }
    }
}
pub(super) fn legacy_tools(
    functions: &[chat::LegacyFunction],
) -> Result<Vec<ct::ToolUnion>, TransformError> {
    functions
        .iter()
        .map(|function| {
            let schema = match &function.parameters {
                Some(parameters) => super::schema::from_chat(parameters)?,
                None => {
                    let mut schema = ct::JsonSchema::builder(ct::JsonSchemaType::Object).build();
                    schema.additional_properties = Some(serde_json::Value::Bool(false));
                    schema
                }
            };
            let mut tool = ct::Tool::builder(schema, function.name.clone()).build();
            tool.description = function.description.clone();
            Ok(ct::ToolUnion::Custom(tool))
        })
        .collect()
}
