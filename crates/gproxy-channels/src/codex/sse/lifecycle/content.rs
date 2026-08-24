use gproxy_protocol::openai::common::ResponseItemLifecycleStatus;
use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ResponseMessageItem, ResponseMessageItemType, ResponseMessageOutputContentPart,
    ResponseOutputMessageItem, ResponseOutputMessageRole, ResponseOutputText,
    ResponseOutputTextType, ResponseReasoningTextPart, ResponseReasoningTextType,
    TypedResponseItem,
};

pub(super) fn clear_started_payload(item: &mut ResponseItem) {
    match item {
        ResponseItem::Message(ResponseMessageItem::Output(message)) => message.content.clear(),
        ResponseItem::Typed(item) => match item.as_mut() {
            TypedResponseItem::Reasoning {
                summary, content, ..
            } => {
                summary.clear();
                *content = None;
            }
            TypedResponseItem::FileSearchCall { .. }
            | TypedResponseItem::ComputerCall { .. }
            | TypedResponseItem::ComputerCallOutput { .. }
            | TypedResponseItem::WebSearchCall { .. }
            | TypedResponseItem::FunctionCall { .. }
            | TypedResponseItem::FunctionCallOutput { .. }
            | TypedResponseItem::ToolSearchCall { .. }
            | TypedResponseItem::ToolSearchOutput { .. }
            | TypedResponseItem::AdditionalTools { .. }
            | TypedResponseItem::Compaction { .. }
            | TypedResponseItem::ImageGenerationCall { .. }
            | TypedResponseItem::CodeInterpreterCall { .. }
            | TypedResponseItem::LocalShellCall { .. }
            | TypedResponseItem::LocalShellCallOutput { .. }
            | TypedResponseItem::ShellCall { .. }
            | TypedResponseItem::ShellCallOutput { .. }
            | TypedResponseItem::ApplyPatchCall { .. }
            | TypedResponseItem::ApplyPatchCallOutput { .. }
            | TypedResponseItem::McpListTools { .. }
            | TypedResponseItem::McpApprovalRequest { .. }
            | TypedResponseItem::McpApprovalResponse { .. }
            | TypedResponseItem::McpCall { .. }
            | TypedResponseItem::CustomToolCall { .. }
            | TypedResponseItem::CustomToolCallOutput { .. }
            | TypedResponseItem::Program { .. }
            | TypedResponseItem::ProgramOutput { .. }
            | TypedResponseItem::MultiAgentCall { .. }
            | TypedResponseItem::MultiAgentCallOutput { .. }
            | TypedResponseItem::AgentMessage { .. }
            | TypedResponseItem::CompactionTrigger { .. }
            | TypedResponseItem::ItemReference { .. } => {}
        },
        ResponseItem::Message(
            ResponseMessageItem::Input(_)
            | ResponseMessageItem::EasyInput(_)
            | ResponseMessageItem::Unknown(_),
        )
        | ResponseItem::Unknown(_) => {}
    }
}

pub(super) fn message_item(id: &str, text: &str) -> ResponseItem {
    ResponseItem::Message(ResponseMessageItem::Output(ResponseOutputMessageItem {
        type_: ResponseMessageItemType::Message,
        id: id.into(),
        role: ResponseOutputMessageRole::Assistant,
        content: vec![ResponseMessageOutputContentPart::OutputText(
            ResponseOutputText {
                type_: ResponseOutputTextType::OutputText,
                annotations: Vec::new(),
                logprobs: None,
                text: text.into(),
                rest: Default::default(),
            },
        )],
        status: ResponseItemLifecycleStatus::InProgress,
        phase: None,
        rest: Default::default(),
    }))
}

pub(super) fn reasoning_item(id: &str, text: &str) -> ResponseItem {
    ResponseItem::Typed(Box::new(TypedResponseItem::Reasoning {
        id: Some(id.into()),
        summary: Vec::new(),
        content: Some(vec![ResponseReasoningTextPart {
            text: text.into(),
            type_: ResponseReasoningTextType::ReasoningText,
            rest: Default::default(),
        }]),
        encrypted_content: None,
        status: Some(ResponseItemLifecycleStatus::InProgress),
        rest: Default::default(),
    }))
}

pub(super) fn append_message(message: &mut ResponseOutputMessageItem, delta: &str) {
    if let Some(ResponseMessageOutputContentPart::OutputText(text)) = message.content.last_mut() {
        text.text.push_str(delta);
    } else {
        message
            .content
            .push(ResponseMessageOutputContentPart::OutputText(
                ResponseOutputText {
                    type_: ResponseOutputTextType::OutputText,
                    annotations: Vec::new(),
                    logprobs: None,
                    text: delta.into(),
                    rest: Default::default(),
                },
            ));
    }
}

pub(super) fn append_reasoning(content: &mut Option<Vec<ResponseReasoningTextPart>>, delta: &str) {
    let parts = content.get_or_insert_with(Vec::new);
    if let Some(part) = parts.last_mut() {
        part.text.push_str(delta);
    } else {
        parts.push(ResponseReasoningTextPart {
            text: delta.into(),
            type_: ResponseReasoningTextType::ReasoningText,
            rest: Default::default(),
        });
    }
}

pub(super) fn item_id(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::Message(ResponseMessageItem::Output(message)) => Some(message.id.clone()),
        ResponseItem::Message(ResponseMessageItem::Input(message)) => message.id.clone(),
        ResponseItem::Typed(item) => match item.as_ref() {
            TypedResponseItem::FunctionCall { id, .. }
            | TypedResponseItem::CustomToolCall { id, .. }
            | TypedResponseItem::Reasoning { id, .. }
            | TypedResponseItem::ApplyPatchCall { id, .. }
            | TypedResponseItem::ShellCall { id, .. } => id.clone(),
            TypedResponseItem::FileSearchCall { .. }
            | TypedResponseItem::ComputerCall { .. }
            | TypedResponseItem::ComputerCallOutput { .. }
            | TypedResponseItem::WebSearchCall { .. }
            | TypedResponseItem::FunctionCallOutput { .. }
            | TypedResponseItem::ToolSearchCall { .. }
            | TypedResponseItem::ToolSearchOutput { .. }
            | TypedResponseItem::AdditionalTools { .. }
            | TypedResponseItem::Compaction { .. }
            | TypedResponseItem::ImageGenerationCall { .. }
            | TypedResponseItem::CodeInterpreterCall { .. }
            | TypedResponseItem::LocalShellCall { .. }
            | TypedResponseItem::LocalShellCallOutput { .. }
            | TypedResponseItem::ShellCallOutput { .. }
            | TypedResponseItem::ApplyPatchCallOutput { .. }
            | TypedResponseItem::McpListTools { .. }
            | TypedResponseItem::McpApprovalRequest { .. }
            | TypedResponseItem::McpApprovalResponse { .. }
            | TypedResponseItem::McpCall { .. }
            | TypedResponseItem::CustomToolCallOutput { .. }
            | TypedResponseItem::Program { .. }
            | TypedResponseItem::ProgramOutput { .. }
            | TypedResponseItem::MultiAgentCall { .. }
            | TypedResponseItem::MultiAgentCallOutput { .. }
            | TypedResponseItem::AgentMessage { .. }
            | TypedResponseItem::CompactionTrigger { .. }
            | TypedResponseItem::ItemReference { .. } => None,
        },
        ResponseItem::Message(ResponseMessageItem::EasyInput(_))
        | ResponseItem::Message(ResponseMessageItem::Unknown(_))
        | ResponseItem::Unknown(_) => None,
    }
}
