use serde_json::Value;

use crate::protocol::{claude, openai};

pub(super) fn claude_tool_use_to_chat_tool_call(
    block: claude::ToolUseBlock,
) -> openai::ChatToolCall {
    openai::ChatToolCall::Function {
        id: block.id,
        function: openai::FunctionCall {
            arguments: serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_owned()),
            name: block.name,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

pub(super) fn claude_response_tool_use_to_chat_tool_call(
    block: claude::ResponseToolUseBlock,
) -> openai::ChatToolCall {
    openai::ChatToolCall::Function {
        id: block.id,
        function: openai::FunctionCall {
            arguments: serde_json::to_string(&block.input).unwrap_or_else(|_| "{}".to_owned()),
            name: block.name,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

pub(super) fn claude_tool_result_to_text(content: Option<claude::ToolResultContent>) -> String {
    match content {
        Some(claude::ToolResultContent::Text(text)) => text,
        Some(claude::ToolResultContent::Blocks(blocks)) => blocks
            .into_iter()
            .filter_map(|block| match block {
                claude::ToolResultContentBlock::Text(block) => Some(block.text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(claude::ToolResultContent::Raw(value)) => value.to_string(),
        None => String::new(),
    }
}

pub(super) fn claude_tools_to_chat(tools: Vec<claude::Tool>) -> Vec<openai::ChatTool> {
    tools
        .into_iter()
        .filter_map(|tool| match tool {
            claude::Tool::Custom(tool) => Some(openai::ChatTool::Function {
                function: openai::FunctionDefinition {
                    name: tool.name,
                    description: tool.description,
                    parameters: Some(claude_schema_to_openai(tool.input_schema)),
                    strict: tool.common.strict,
                    extra: Default::default(),
                },
                extra: Default::default(),
            }),
            _ => None,
        })
        .collect()
}

fn claude_schema_to_openai(schema: claude::JsonSchema) -> openai::JsonSchema {
    let mut parameters = openai::JsonSchema::default();
    parameters.insert("type".to_owned(), Value::String("object".to_owned()));
    if !schema.properties.is_empty() {
        parameters.insert(
            "properties".to_owned(),
            Value::Object(schema.properties.into_iter().collect()),
        );
    }
    if !schema.required.is_empty() {
        parameters.insert(
            "required".to_owned(),
            Value::Array(schema.required.into_iter().map(Value::String).collect()),
        );
    }
    parameters
}

pub(super) fn claude_tool_choice_to_chat(
    choice: Option<claude::ToolChoice>,
) -> Option<openai::ChatToolChoice> {
    match choice? {
        claude::ToolChoice::Auto(_) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto))
        }
        claude::ToolChoice::Any(_) => Some(openai::ChatToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        claude::ToolChoice::None(_) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None))
        }
        claude::ToolChoice::Tool(choice) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Function {
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::NamedTool {
                    name: choice.name,
                    extra: Default::default(),
                },
                extra: Default::default(),
            },
        )),
        claude::ToolChoice::Unknown(_) => None,
    }
}
