use gproxy_protocol::openai;

pub(super) fn flush_message(
    output: &mut Vec<openai::ResponseItem>,
    parts: &mut Vec<openai::ResponseMessageOutputContentPart>,
    id: &mut Option<String>,
    response_id: &str,
    message_index: &mut u32,
) {
    if parts.is_empty() {
        return;
    }
    output.push(openai::ResponseItem::Message(
        openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: id.take().unwrap_or_else(|| {
                let id = format!("msg_{response_id}_{}", *message_index);
                *message_index = message_index.saturating_add(1);
                id
            }),
            role: openai::ResponseOutputMessageRole::Assistant,
            content: std::mem::take(parts),
            status: openai::ResponseItemLifecycleStatus::Completed,
            phase: None,
            rest: Default::default(),
        }),
    ));
}

pub(super) fn reasoning(
    id: Option<String>,
    text: Option<String>,
    encrypted_content: Option<String>,
    _extensions: serde_json::Map<String, serde_json::Value>,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
        id,
        summary: Vec::new(),
        content: text.map(|text| {
            vec![openai::ResponseReasoningTextPart {
                type_: openai::ResponseReasoningTextType::ReasoningText,
                text,
                rest: Default::default(),
            }]
        }),
        encrypted_content,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        rest: Default::default(),
    }))
}
