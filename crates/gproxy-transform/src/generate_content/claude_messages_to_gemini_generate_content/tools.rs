use gproxy_protocol::{claude, gemini};

use crate::TransformError;

mod native;

pub(super) fn definitions(
    tools: Option<Vec<claude::Tool>>,
) -> Result<Vec<gemini::Tool>, TransformError> {
    let Some(tools) = tools else {
        return Ok(Vec::new());
    };
    let mut output = gemini::Tool::default();
    for tool in tools {
        match tool {
            claude::Tool::Custom(tool) => {
                if tool.eager_input_streaming.is_some()
                    || !tool.rest.is_empty()
                    || tool.common.allowed_callers.is_some()
                    || tool.common.cache_control.is_some()
                    || tool.common.defer_loading.is_some()
                    || !tool.common.input_examples.is_empty()
                    || tool.common.strict.is_some()
                    || !tool.common.rest.is_empty()
                {
                    return Err(TransformError::unsupported(
                        "Claude custom tool",
                        "fields without a Gemini counterpart",
                    ));
                }
                let description = tool.description.ok_or_else(|| {
                    TransformError::shape("Claude tool", "description is missing")
                })?;
                output
                    .function_declarations
                    .get_or_insert_with(Vec::new)
                    .push(gemini::FunctionDeclaration {
                        name: tool.name,
                        description,
                        behavior: None,
                        parameters: None,
                        parameters_json_schema: Some(serde_json::to_value(tool.input_schema)?),
                        response: None,
                        response_json_schema: None,
                        rest: Default::default(),
                    });
            }
            claude::Tool::Command(command) if native::command(&command) => {
                output.code_execution = Some(gemini::CodeExecution::default());
            }
            claude::Tool::TextEditor(editor) if native::editor(&editor) => {
                output.code_execution = Some(gemini::CodeExecution::default());
            }
            claude::Tool::WebSearch(_) => {
                return Err(TransformError::unsupported(
                    "Claude web-search tool",
                    "native definition mapping is not lossless",
                ));
            }
            claude::Tool::Unknown(raw) => {
                return Err(TransformError::unsupported("Claude tool", raw.to_string()));
            }
            other => {
                return Err(TransformError::unsupported(
                    "Claude tool",
                    serde_json::to_string(&other)?,
                ));
            }
        }
    }
    Ok((!is_empty(&output)).then_some(output).into_iter().collect())
}

pub(super) fn choice(choice: Option<claude::ToolChoice>) -> Option<gemini::ToolConfig> {
    let (mode, allowed_function_names, rest) = match choice? {
        claude::ToolChoice::Auto(choice) => (
            gemini::FunctionCallingModeKnown::Auto,
            Vec::new(),
            choice.rest,
        ),
        claude::ToolChoice::Any(choice) => (
            gemini::FunctionCallingModeKnown::Any,
            Vec::new(),
            choice.rest,
        ),
        claude::ToolChoice::None(choice) => (
            gemini::FunctionCallingModeKnown::None,
            Vec::new(),
            choice.rest,
        ),
        claude::ToolChoice::Tool(choice) => (
            gemini::FunctionCallingModeKnown::Any,
            vec![choice.name],
            choice.rest,
        ),
        claude::ToolChoice::Unknown(_) => return None,
        _ => return None,
    };
    Some(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(mode)),
            allowed_function_names: (!allowed_function_names.is_empty())
                .then_some(allowed_function_names),
            rest,
        }),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        rest: Default::default(),
    })
}

pub(super) fn is_native_name(name: &str) -> bool {
    matches!(
        name,
        "bash" | "str_replace_editor" | "str_replace_based_edit_tool"
    )
}

pub(super) fn is_server_native_name(name: &claude::ServerToolUseName) -> bool {
    matches!(
        name,
        claude::ServerToolUseName::Known(
            claude::ServerToolUseNameKnown::CodeExecution
                | claude::ServerToolUseNameKnown::BashCodeExecution
                | claude::ServerToolUseNameKnown::TextEditorCodeExecution
        )
    )
}

fn is_empty(tool: &gemini::Tool) -> bool {
    tool.function_declarations.is_none()
        && tool.google_search_retrieval.is_none()
        && tool.code_execution.is_none()
        && tool.google_search.is_none()
        && tool.computer_use.is_none()
        && tool.url_context.is_none()
        && tool.file_search.is_none()
        && tool.mcp_servers.is_none()
        && tool.google_maps.is_none()
        && tool.rest.is_empty()
}
