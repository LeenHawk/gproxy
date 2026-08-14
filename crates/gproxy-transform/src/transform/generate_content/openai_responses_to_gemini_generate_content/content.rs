use std::collections::BTreeMap;

use crate::protocol::{gemini, openai};

pub(in crate::transform) fn response_input_to_gemini_contents(
    input: Option<openai::ResponseInput>,
) -> Vec<gemini::Content> {
    match input {
        Some(openai::ResponseInput::Text(text)) => vec![content(
            gemini::ContentRoleKnown::User,
            vec![text_part(text, false, None)],
        )],
        Some(openai::ResponseInput::Items(items)) => {
            let mut function_names = BTreeMap::new();
            items
                .into_iter()
                .filter_map(|item| {
                    let function_name = match &item {
                        openai::ResponseItem::Typed(
                            openai::TypedResponseItem::FunctionCall { call_id, name, .. }
                            | openai::TypedResponseItem::CustomToolCall { call_id, name, .. },
                        ) => {
                            function_names.insert(call_id.clone(), name.clone());
                            None
                        }
                        openai::ResponseItem::Typed(
                            openai::TypedResponseItem::FunctionCallOutput { call_id, .. }
                            | openai::TypedResponseItem::CustomToolCallOutput { call_id, .. },
                        ) => function_names.get(call_id).cloned(),
                        _ => None,
                    };
                    response_item_to_gemini_content_with_name(item, function_name)
                })
                .collect()
        }
        None => Vec::new(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(super) fn response_item_to_gemini_content(
    item: openai::ResponseItem,
) -> Option<gemini::Content> {
    response_item_to_gemini_content_with_name(item, None)
}

fn response_item_to_gemini_content_with_name(
    item: openai::ResponseItem,
    function_name: Option<String>,
) -> Option<gemini::Content> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(message)) => {
            let role = match message.role {
                openai::ResponseEasyInputMessageRole::Assistant => gemini::ContentRoleKnown::Model,
                openai::ResponseEasyInputMessageRole::System
                | openai::ResponseEasyInputMessageRole::Developer => {
                    gemini::ContentRoleKnown::System
                }
                openai::ResponseEasyInputMessageRole::User => gemini::ContentRoleKnown::User,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            let parts = match message.content {
                openai::ResponseEasyInputContent::Text(text) => vec![text_part(text, false, None)],
                openai::ResponseEasyInputContent::Parts(parts) => {
                    parts.into_iter().filter_map(input_part_to_gemini).collect()
                }
                openai::ResponseEasyInputContent::OutputParts(parts) => {
                    parts.into_iter().map(output_part_to_gemini).collect()
                }
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            (!parts.is_empty()).then(|| content(role, parts))
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Input(message)) => {
            let role = match message.role {
                openai::ResponseInputMessageRole::User => gemini::ContentRoleKnown::User,
                openai::ResponseInputMessageRole::System
                | openai::ResponseInputMessageRole::Developer => gemini::ContentRoleKnown::System,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            let parts = message
                .content
                .into_iter()
                .filter_map(input_part_to_gemini)
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| content(role, parts))
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            let parts = message
                .content
                .into_iter()
                .map(output_part_to_gemini)
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| content(gemini::ContentRoleKnown::Model, parts))
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            summary,
            content: reasoning,
            encrypted_content,
            ..
        }) => {
            let text = reasoning
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .chain(summary.into_iter().map(|part| part.text))
                .collect::<Vec<_>>()
                .join("");
            Some(content(
                gemini::ContentRoleKnown::Model,
                vec![text_part(text, true, encrypted_content)],
            ))
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            call_id,
            name,
            arguments,
            mut extra,
            ..
        }) => Some(function_call_content(
            call_id,
            name,
            arguments,
            take_thought_signature(&mut extra),
        )),
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            name,
            input,
            mut extra,
            ..
        }) => Some(function_call_content(
            call_id,
            name,
            input,
            take_thought_signature(&mut extra),
        )),
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            ..
        })
        | openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCallOutput {
            call_id,
            output,
            ..
        }) => Some(function_response_content(
            call_id.clone(),
            function_name.unwrap_or(call_id),
            output_text(output),
        )),
        _ => None,
    }
}

fn input_part_to_gemini(part: openai::ResponseInputContentPart) -> Option<gemini::Part> {
    match part {
        openai::ResponseInputContentPart::InputText { text, .. } => {
            Some(text_part(text, false, None))
        }
        openai::ResponseInputContentPart::InputImage {
            image_url: Some(file_uri),
            ..
        }
        | openai::ResponseInputContentPart::InputFile {
            file_url: Some(file_uri),
            ..
        } => Some(crate::protocol::wire!(gemini::Part {
            data: Some(gemini::PartData::FileData {
                file_data: crate::protocol::wire!(gemini::FileData {
                    mime_type: None,
                    file_uri,
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        })),
        _ => None,
    }
}

fn output_part_to_gemini(part: openai::ResponseMessageOutputContentPart) -> gemini::Part {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText { text, .. } => {
            text_part(text, false, None)
        }
        openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
            text_part(refusal, false, None)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn function_call_content(
    call_id: String,
    name: String,
    arguments: String,
    thought_signature: Option<String>,
) -> gemini::Content {
    content(
        gemini::ContentRoleKnown::Model,
        vec![crate::protocol::wire!(gemini::Part {
            thought_signature,
            data: Some(gemini::PartData::FunctionCall {
                function_call: crate::protocol::wire!(gemini::FunctionCall {
                    id: Some(call_id),
                    name,
                    args: serde_json::from_str(&arguments).ok(),
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        })],
    )
}

fn take_thought_signature(extra: &mut openai::Extra) -> Option<String> {
    extra
        .remove("thought_signature")
        .or_else(|| extra.remove("thoughtSignature"))
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn function_response_content(call_id: String, name: String, output: String) -> gemini::Content {
    let mut response = gemini::JsonMap::new();
    response.insert("output".to_owned(), serde_json::Value::String(output));
    content(
        gemini::ContentRoleKnown::User,
        vec![crate::protocol::wire!(gemini::Part {
            data: Some(gemini::PartData::FunctionResponse {
                function_response: crate::protocol::wire!(gemini::FunctionResponse {
                    id: Some(call_id.clone()),
                    name,
                    response,
                    parts: Vec::new(),
                    will_continue: None,
                    scheduling: None,
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        })],
    )
}

fn output_text(output: openai::ResponseOutput) -> String {
    match output {
        openai::ResponseOutput::Text(text) => text,
        openai::ResponseOutput::Parts(parts) => serde_json::to_string(&parts).unwrap_or_default(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn text_part(text: String, thought: bool, thought_signature: Option<String>) -> gemini::Part {
    crate::protocol::wire!(gemini::Part {
        thought: thought.then_some(true),
        thought_signature,
        data: (!text.is_empty()).then_some(gemini::PartData::Text { text }),
        ..Default::default()
    })
}

fn content(role: gemini::ContentRoleKnown, parts: Vec<gemini::Part>) -> gemini::Content {
    crate::protocol::wire!(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(role)),
        extra: Default::default(),
    })
}
