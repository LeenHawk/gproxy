use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::media;

pub(super) fn message(
    message: openai::ResponseMessageItem,
) -> Result<Option<gemini::Content>, TransformError> {
    match message {
        openai::ResponseMessageItem::EasyInput(message) => {
            let role = easy_role(message.role)?;
            let parts = match message.content {
                openai::ResponseEasyInputContent::Text(text) => vec![text_part(text, false, None)],
                openai::ResponseEasyInputContent::Parts(parts) => parts
                    .into_iter()
                    .map(media::input_part)
                    .collect::<Result<_, _>>()?,
                openai::ResponseEasyInputContent::OutputParts(parts) => {
                    parts.into_iter().filter_map(output_part).collect()
                }
                openai::ResponseEasyInputContent::Unknown(_) => return Ok(None),
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            };
            Ok(Some(content(role, parts)))
        }
        openai::ResponseMessageItem::Input(message) => {
            let role = match message.role {
                openai::ResponseInputMessageRole::User => gemini::ContentRoleKnown::User,
                openai::ResponseInputMessageRole::System
                | openai::ResponseInputMessageRole::Developer => gemini::ContentRoleKnown::System,
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            };
            Ok(Some(content(
                role,
                message
                    .content
                    .into_iter()
                    .map(media::input_part)
                    .collect::<Result<_, _>>()?,
            )))
        }
        openai::ResponseMessageItem::Output(message) => Ok(Some(content(
            gemini::ContentRoleKnown::Model,
            message
                .content
                .into_iter()
                .filter_map(output_part)
                .collect(),
        ))),
        openai::ResponseMessageItem::Unknown(_) => Ok(None),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

pub(super) fn text_content(role: gemini::ContentRoleKnown, text: String) -> gemini::Content {
    content(role, vec![text_part(text, false, None)])
}

pub(super) fn text_part(text: String, thought: bool, signature: Option<String>) -> gemini::Part {
    crate::wire!(gemini::Part {
        thought: thought.then_some(true),
        thought_signature: signature,
        data: Some(gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        ..Default::default()
    })
}

fn output_part(part: openai::ResponseMessageOutputContentPart) -> Option<gemini::Part> {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText(part) => {
            Some(text_part(part.text, false, None))
        }
        openai::ResponseMessageOutputContentPart::Refusal(part) => {
            Some(text_part(part.refusal, false, None))
        }
        openai::ResponseMessageOutputContentPart::Unknown(_) => None,
        #[cfg(not(feature = "exhaustive"))]
        _ => None,
    }
}

fn easy_role(
    role: openai::ResponseEasyInputMessageRole,
) -> Result<gemini::ContentRoleKnown, TransformError> {
    Ok(match role {
        openai::ResponseEasyInputMessageRole::Assistant => gemini::ContentRoleKnown::Model,
        openai::ResponseEasyInputMessageRole::System
        | openai::ResponseEasyInputMessageRole::Developer => gemini::ContentRoleKnown::System,
        openai::ResponseEasyInputMessageRole::User => gemini::ContentRoleKnown::User,
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(TransformError::unsupported(
                "OpenAI Responses message role",
                "unrecognized external variant",
            ));
        }
    })
}

fn content(role: gemini::ContentRoleKnown, parts: Vec<gemini::Part>) -> gemini::Content {
    crate::wire!(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(role)),
        rest: Default::default(),
    })
}
