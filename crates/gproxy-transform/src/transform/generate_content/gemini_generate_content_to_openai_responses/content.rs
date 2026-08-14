use crate::protocol::{gemini, openai};

use super::super::common;

pub(super) fn gemini_contents_to_response_items(
    contents: Vec<gemini::Content>,
) -> Vec<openai::ResponseItem> {
    contents
        .into_iter()
        .flat_map(gemini_content_to_response_items)
        .collect()
}

pub(super) fn gemini_content_to_response_output(
    content: gemini::Content,
) -> Vec<openai::ResponseOutputItem> {
    let mut output = Vec::new();
    let mut text_parts = Vec::new();
    for part in content.parts {
        let signature = part.thought_signature;
        match part.data {
            Some(gemini::PartData::Text { text }) if part.thought == Some(true) => {
                output.push(openai::ResponseOutputItem::new(reasoning_item(
                    (!text.is_empty()).then_some(text),
                    signature,
                )));
            }
            None if signature.is_some() => output.push(openai::ResponseOutputItem::new(
                reasoning_item(None, signature),
            )),
            Some(gemini::PartData::Text { text }) => {
                if signature.is_some() {
                    output.push(openai::ResponseOutputItem::new(reasoning_item(
                        None, signature,
                    )));
                }
                text_parts.push(openai::ResponseMessageOutputContentPart::OutputText {
                    annotations: Vec::new(),
                    logprobs: None,
                    text,
                    extra: Default::default(),
                });
            }
            Some(gemini::PartData::FunctionCall { function_call }) => {
                let call_id = function_call
                    .id
                    .unwrap_or_else(|| format!("call_{}", function_call.name));
                let item_id = common::response_function_call_item_id(&call_id);
                output.push(openai::ResponseOutputItem::new(
                    openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                        arguments: serde_json::to_string(&function_call.args.unwrap_or_default())
                            .unwrap_or_else(|_| "{}".to_owned()),
                        call_id: call_id.clone(),
                        name: function_call.name,
                        id: Some(item_id),
                        caller: None,
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: thought_signature_extra(signature),
                    }),
                ));
            }
            _ => {}
        }
    }
    if !text_parts.is_empty() {
        output.push(openai::ResponseOutputItem::new(
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                crate::protocol::wire!(openai::ResponseOutputMessageItem {
                    type_: openai::ResponseMessageItemType::Message,
                    id: "message".to_owned(),
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content: text_parts,
                    status: openai::ResponseItemLifecycleStatus::Completed,
                    phase: None,
                    extra: Default::default(),
                }),
            )),
        ));
    }
    output
}

fn gemini_content_to_response_items(content: gemini::Content) -> Vec<openai::ResponseItem> {
    let role = content.role;
    let mut items = Vec::new();
    let mut text_parts = Vec::new();

    for part in content.parts {
        let signature = part.thought_signature;
        match part.data {
            Some(gemini::PartData::Text { text }) if part.thought == Some(true) => {
                items.push(reasoning_item(
                    (!text.is_empty()).then_some(text),
                    signature,
                ));
            }
            None if signature.is_some() => items.push(reasoning_item(None, signature)),
            Some(gemini::PartData::Text { text }) => {
                if signature.is_some() {
                    items.push(reasoning_item(None, signature));
                }
                text_parts.push(text);
            }
            Some(gemini::PartData::FunctionCall { function_call }) => {
                let call_id = function_call
                    .id
                    .unwrap_or_else(|| format!("call_{}", function_call.name));
                let item_id = common::response_function_call_item_id(&call_id);
                items.push(openai::ResponseItem::Typed(
                    openai::TypedResponseItem::FunctionCall {
                        arguments: serde_json::to_string(&function_call.args.unwrap_or_default())
                            .unwrap_or_else(|_| "{}".to_owned()),
                        call_id: call_id.clone(),
                        name: function_call.name,
                        id: Some(item_id),
                        caller: None,
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: thought_signature_extra(signature),
                    },
                ));
            }
            Some(gemini::PartData::FunctionResponse { function_response }) => {
                let call_id = function_response
                    .id
                    .unwrap_or_else(|| function_response.name.clone());
                items.push(openai::ResponseItem::Typed(
                    openai::TypedResponseItem::FunctionCallOutput {
                        call_id,
                        output: openai::ResponseOutput::Text(
                            serde_json::to_string(&function_response.response).unwrap_or_default(),
                        ),
                        id: None,
                        caller: None,
                        name: None,
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        created_by: None,
                        extra: Default::default(),
                    },
                ));
            }
            _ => {}
        }
    }

    if !text_parts.is_empty() {
        let text = text_parts.join("");
        let item = match role {
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => {
                openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
                    crate::protocol::wire!(openai::ResponseEasyInputMessageItem {
                        type_: Some(openai::ResponseMessageItemType::Message),
                        role: openai::ResponseEasyInputMessageRole::Assistant,
                        content: openai::ResponseEasyInputContent::Text(text),
                        phase: None,
                        extra: Default::default(),
                    }),
                ))
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
                easy_message(openai::ResponseEasyInputMessageRole::System, text)
            }
            _ => easy_message(openai::ResponseEasyInputMessageRole::User, text),
        };
        items.insert(0, item);
    }
    items
}

fn thought_signature_extra(signature: Option<String>) -> openai::Extra {
    let mut extra = openai::Extra::new();
    if let Some(signature) = signature {
        extra.insert(
            "thought_signature".to_owned(),
            serde_json::Value::String(signature),
        );
    }
    extra
}

fn easy_message(role: openai::ResponseEasyInputMessageRole, text: String) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
        crate::protocol::wire!(openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content: openai::ResponseEasyInputContent::Text(text),
            phase: None,
            extra: Default::default(),
        }),
    ))
}

fn reasoning_item(text: Option<String>, encrypted_content: Option<String>) -> openai::ResponseItem {
    let id_source = encrypted_content
        .as_deref()
        .or(text.as_deref())
        .unwrap_or_default();
    openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
        id: Some(
            crate::transform::generate_content::common::id::response_reasoning_item_id(id_source),
        ),
        summary: Vec::new(),
        content: text.map(|text| {
            vec![crate::protocol::wire!(openai::ResponseReasoningTextPart {
                text,
                type_: openai::ResponseReasoningTextType::ReasoningText,
                extra: Default::default(),
            })]
        }),
        encrypted_content,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}
