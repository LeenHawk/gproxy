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
                let description = tool.description.unwrap_or_default();
                output
                    .function_declarations
                    .get_or_insert_with(Vec::new)
                    .push(gemini::FunctionDeclaration {
                        name: tool.name,
                        description,
                        behavior: None,
                        parameters: None,
                        parameters_json_schema: Some(schema_value(tool.input_schema)?),
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
            claude::Tool::WebSearch(_) | claude::Tool::Unknown(_) => {}
            _future => {}
        }
    }
    Ok((!is_empty(&output)).then_some(output).into_iter().collect())
}

pub(super) fn choice(choice: Option<claude::ToolChoice>) -> Option<gemini::ToolConfig> {
    let (mode, allowed_function_names) = match choice? {
        claude::ToolChoice::Auto(_) => (gemini::FunctionCallingModeKnown::Auto, Vec::new()),
        claude::ToolChoice::Any(_) => (gemini::FunctionCallingModeKnown::Any, Vec::new()),
        claude::ToolChoice::None(_) => (gemini::FunctionCallingModeKnown::None, Vec::new()),
        claude::ToolChoice::Tool(choice) => {
            (gemini::FunctionCallingModeKnown::Any, vec![choice.name])
        }
        claude::ToolChoice::Unknown(_) => return None,
        _ => return None,
    };
    Some(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(mode)),
            allowed_function_names: (!allowed_function_names.is_empty())
                .then_some(allowed_function_names),
            rest: Default::default(),
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
}

fn schema_value(schema: claude::JsonSchema) -> Result<serde_json::Value, TransformError> {
    let type_ = match schema.type_ {
        claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object) => "object",
        claude::JsonSchemaObjectType::Unknown(value) => {
            return Err(TransformError::unsupported(
                "Claude tool schema type",
                value,
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Claude tool schema type",
                "future type",
            ));
        }
    };
    Ok(serde_json::json!({
        "type": type_,
        "properties": schema.properties,
        "required": schema.required,
    }))
}
