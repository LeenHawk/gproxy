use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::media;

pub(super) fn message(
    message: openai::ResponseMessageItem,
) -> Result<gemini::Content, TransformError> {
    match message {
        openai::ResponseMessageItem::EasyInput(message) => {
            let role = easy_role(message.role);
            let parts = match message.content {
                openai::ResponseEasyInputContent::Text(text) => vec![text_part(text, false, None)],
                openai::ResponseEasyInputContent::Parts(parts) => parts
                    .into_iter()
                    .map(media::input_part)
                    .collect::<Result<_, _>>()?,
                openai::ResponseEasyInputContent::OutputParts(parts) => {
                    parts.into_iter().map(output_part).collect()
                }
                openai::ResponseEasyInputContent::Unknown(raw) => vec![raw_part(raw)],
            };
            Ok(content(role, parts, message.rest))
        }
        openai::ResponseMessageItem::Input(message) => {
            let role = match message.role {
                openai::ResponseInputMessageRole::User => gemini::ContentRoleKnown::User,
                openai::ResponseInputMessageRole::System
                | openai::ResponseInputMessageRole::Developer => gemini::ContentRoleKnown::System,
            };
            let mut rest = message.rest;
            preserve_id(&mut rest, message.id);
            Ok(content(
                role,
                message
                    .content
                    .into_iter()
                    .map(media::input_part)
                    .collect::<Result<_, _>>()?,
                rest,
            ))
        }
        openai::ResponseMessageItem::Output(message) => {
            let mut rest = message.rest;
            preserve_id(&mut rest, message.id);
            Ok(content(
                gemini::ContentRoleKnown::Model,
                message.content.into_iter().map(output_part).collect(),
                rest,
            ))
        }
        openai::ResponseMessageItem::Unknown(raw) => Ok(gemini::Content {
            parts: vec![raw_part(raw)],
            role: None,
            rest: Default::default(),
        }),
    }
}

pub(super) fn text_content(role: gemini::ContentRoleKnown, text: String) -> gemini::Content {
    content(role, vec![text_part(text, false, None)], Default::default())
}

pub(super) fn text_part(text: String, thought: bool, signature: Option<String>) -> gemini::Part {
    gemini::Part {
        thought: thought.then_some(true),
        thought_signature: signature,
        data: Some(gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        ..Default::default()
    }
}

fn output_part(part: openai::ResponseMessageOutputContentPart) -> gemini::Part {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText(part) => {
            let mut output = text_part(part.text, false, None);
            output.rest = part.rest;
            output
        }
        openai::ResponseMessageOutputContentPart::Refusal(part) => {
            let mut output = text_part(part.refusal, false, None);
            output.rest = part.rest;
            output
        }
        openai::ResponseMessageOutputContentPart::Unknown(raw) => raw_part(raw),
    }
}

fn raw_part(raw: serde_json::Value) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::Raw(raw)),
        ..Default::default()
    }
}

fn easy_role(role: openai::ResponseEasyInputMessageRole) -> gemini::ContentRoleKnown {
    match role {
        openai::ResponseEasyInputMessageRole::Assistant => gemini::ContentRoleKnown::Model,
        openai::ResponseEasyInputMessageRole::System
        | openai::ResponseEasyInputMessageRole::Developer => gemini::ContentRoleKnown::System,
        openai::ResponseEasyInputMessageRole::User => gemini::ContentRoleKnown::User,
    }
}

fn content(
    role: gemini::ContentRoleKnown,
    parts: Vec<gemini::Part>,
    rest: gemini::JsonMap,
) -> gemini::Content {
    gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(role)),
        rest,
    }
}

fn preserve_id(rest: &mut gemini::JsonMap, id: Option<String>) {
    if let Some(id) = id {
        rest.insert("openai_item_id".into(), id.into());
    }
}
