use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn text_part(text: String, thought: bool, rest: gemini::ExtraFields) -> gemini::Part {
    gemini::Part {
        thought: thought.then_some(true),
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        metadata: None,
        rest,
    }
}

pub(crate) fn function_call(
    id: Option<String>,
    name: String,
    arguments: &str,
    rest: gemini::ExtraFields,
) -> Result<gemini::Part, TransformError> {
    let args = serde_json::from_str(arguments).ok();
    Ok(gemini::Part {
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id,
                name,
                args,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest,
        ..Default::default()
    })
}

pub(super) fn user_parts(
    content: openai::ChatContent,
) -> Result<Vec<gemini::Part>, TransformError> {
    match content {
        openai::ChatContent::Text(text) => Ok(non_empty_text(text).into_iter().collect()),
        openai::ChatContent::Parts(parts) => parts
            .into_iter()
            .map(super::media::user_part)
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.into_iter().flatten().collect()),
        openai::ChatContent::Unknown(raw) => Err(TransformError::unsupported(
            "Chat user content",
            raw.to_string(),
        )),
    }
}

pub(super) fn text_content(content: openai::ChatTextContent) -> Result<String, TransformError> {
    match content {
        openai::ChatTextContent::Text(text) => Ok(text),
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text(part) => Ok(part.text),
                openai::ChatTextContentPart::Unknown(raw) => Err(TransformError::unsupported(
                    "Chat text part",
                    raw.to_string(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.join("")),
        openai::ChatTextContent::Unknown(raw) => Err(TransformError::unsupported(
            "Chat text content",
            raw.to_string(),
        )),
    }
}

pub(super) fn assistant_parts(
    content: openai::ChatAssistantContent,
) -> Result<Vec<gemini::Part>, TransformError> {
    match content {
        openai::ChatAssistantContent::Text(text) => Ok(non_empty_text(text).into_iter().collect()),
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatAssistantContentPart::Text(part) => Ok(non_empty_text(part.text)),
                openai::ChatAssistantContentPart::Refusal(part) => Ok(non_empty_text(part.refusal)),
                openai::ChatAssistantContentPart::Unknown(raw) => Err(TransformError::unsupported(
                    "Chat assistant part",
                    raw.to_string(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|parts| parts.into_iter().flatten().collect()),
        openai::ChatAssistantContent::Unknown(raw) => Err(TransformError::unsupported(
            "Chat assistant content",
            raw.to_string(),
        )),
    }
}

fn non_empty_text(text: String) -> Option<gemini::Part> {
    (!text.is_empty()).then(|| text_part(text, false, Default::default()))
}
