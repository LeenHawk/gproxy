use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::parts::text_part;

pub(super) fn convert(content: gemini::Content) -> Result<openai::ChatTextContent, TransformError> {
    let mut output = Vec::new();
    for part in content.parts {
        match part.data {
            Some(gemini::PartData::Text { text, .. }) => output.push(text_part(text)),
            Some(gemini::PartData::Raw(_)) => {}
            Some(other) => {
                return Err(TransformError::unsupported(
                    "Gemini system instruction",
                    serde_json::to_string(&other)?,
                ));
            }
            None => {}
        }
    }
    if output.is_empty() {
        return Err(TransformError::shape(
            "Gemini system instruction",
            "no representable parts",
        ));
    }
    let text = if output.len() == 1 {
        match output.pop() {
            Some(openai::ChatTextContentPart::Text(part)) => {
                openai::ChatTextContent::Text(part.text)
            }
            Some(part) => openai::ChatTextContent::Parts(vec![part]),
            None => return Err(TransformError::shape("Gemini system instruction", "empty")),
        }
    } else {
        openai::ChatTextContent::Parts(output)
    };
    Ok(text)
}
