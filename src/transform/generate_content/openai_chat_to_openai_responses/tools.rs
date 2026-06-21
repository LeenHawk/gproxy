use crate::protocol::openai;

use super::super::common;

#[derive(Debug, Clone, Copy)]
pub(super) enum ResponseToolOutputKind {
    Function,
    Custom,
}

pub(super) fn chat_tools_to_response_tools(
    tools: Option<Vec<openai::ChatTool>>,
) -> Option<Vec<openai::ResponseTool>> {
    let tools = tools?
        .into_iter()
        .map(chat_tool_to_response_tool)
        .collect::<Vec<_>>();
    (!tools.is_empty()).then_some(tools)
}

pub(super) fn chat_tool_choice_to_response_tool_choice(
    choice: Option<openai::ChatToolChoice>,
) -> Option<openai::ResponseToolChoice> {
    Some(match choice? {
        openai::ChatToolChoice::Mode(mode) => openai::ResponseToolChoice::Mode(mode),
        openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Function {
            function, ..
        }) => openai::ResponseToolChoice::Function(openai::ResponseFunctionToolChoice {
            type_: openai::FunctionToolChoiceType::Function,
            name: function.name,
            extra: Default::default(),
        }),
        openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Custom { custom, .. }) => {
            openai::ResponseToolChoice::Custom(openai::ResponseCustomToolChoice {
                type_: openai::CustomToolChoiceType::Custom,
                name: custom.name,
                extra: Default::default(),
            })
        }
        openai::ChatToolChoice::Allowed(_) => return None,
    })
}

pub(super) fn chat_tool_call_to_response_item(call: openai::ChatToolCall) -> openai::ResponseItem {
    chat_tool_call_to_response_item_and_output_kind(call).0
}

pub(super) fn chat_tool_call_to_response_item_and_output_kind(
    call: openai::ChatToolCall,
) -> (openai::ResponseItem, String, String, ResponseToolOutputKind) {
    match call {
        openai::ChatToolCall::Function { id, function, .. } => {
            let call_id = common::response_call_id(&id);
            let item = openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                arguments: function.arguments,
                call_id: call_id.clone(),
                name: function.name,
                id: Some(common::response_function_call_item_id(&id)),
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                extra: Default::default(),
            });
            (item, id, call_id, ResponseToolOutputKind::Function)
        }
        openai::ChatToolCall::Custom { id, custom, .. } => {
            let call_id = common::response_call_id(&id);
            let item = openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                call_id: call_id.clone(),
                input: custom.input,
                name: custom.name,
                id: None,
                namespace: None,
                extra: Default::default(),
            });
            (item, id, call_id, ResponseToolOutputKind::Custom)
        }
    }
}

pub(super) fn tool_output_item(
    kind: ResponseToolOutputKind,
    call_id: String,
    output: openai::ResponseOutput,
) -> openai::ResponseItem {
    match kind {
        ResponseToolOutputKind::Function => {
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCallOutput {
                call_id,
                output,
                id: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                extra: Default::default(),
            })
        }
        ResponseToolOutputKind::Custom => {
            openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCallOutput {
                call_id,
                output,
                id: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                extra: Default::default(),
            })
        }
    }
}

pub(super) fn legacy_function_call_to_response_item(
    call: openai::FunctionCall,
) -> openai::ResponseItem {
    let call_id = legacy_function_call_id(&call.name);
    openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
        arguments: call.arguments,
        call_id: call_id.clone(),
        name: call.name,
        id: Some(common::response_function_call_item_id(&call_id)),
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}

pub(super) fn legacy_function_call_id(name: &str) -> String {
    format!("call_{name}")
}

fn chat_tool_to_response_tool(tool: openai::ChatTool) -> openai::ResponseTool {
    match tool {
        openai::ChatTool::Function { function, .. } => openai::ResponseTool::Function {
            name: function.name,
            parameters: function.parameters.unwrap_or_default(),
            strict: function.strict.unwrap_or(false),
            defer_loading: None,
            description: function.description,
            extra: Default::default(),
        },
        openai::ChatTool::Custom { custom, .. } => openai::ResponseTool::Custom {
            name: custom.name,
            defer_loading: None,
            description: custom.description,
            format: custom.format,
            extra: Default::default(),
        },
    }
}
