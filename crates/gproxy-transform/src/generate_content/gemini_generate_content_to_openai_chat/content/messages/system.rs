use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::parts::{merge, text_part};

pub(super) fn convert(
    content: gemini::Content,
) -> Result<(openai::ChatTextContent, openai::Rest), TransformError> {
    let mut output = Vec::new();
    for part in content.parts {
        let rest = part_rest(&part)?;
        match part.data {
            Some(gemini::PartData::Text {
                text,
                rest: data_rest,
            }) => output.push(text_part(text, merge(rest, data_rest))),
            Some(gemini::PartData::Raw(raw)) => {
                output.push(openai::ChatTextContentPart::Unknown(raw));
            }
            Some(other) => {
                return Err(TransformError::unsupported(
                    "Gemini system instruction",
                    serde_json::to_string(&other)?,
                ));
            }
            None if rest.is_empty() => {}
            None => output.push(openai::ChatTextContentPart::Unknown(
                serde_json::Value::Object(rest),
            )),
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
            Some(openai::ChatTextContentPart::Text(part)) if part.rest.is_empty() => {
                openai::ChatTextContent::Text(part.text)
            }
            Some(part) => openai::ChatTextContent::Parts(vec![part]),
            None => return Err(TransformError::shape("Gemini system instruction", "empty")),
        }
    } else {
        openai::ChatTextContent::Parts(output)
    };
    Ok((text, content.rest))
}

fn part_rest(part: &gemini::Part) -> Result<openai::Rest, TransformError> {
    let mut rest = part.rest.clone();
    preserve(&mut rest, "gemini_thought", &part.thought)?;
    preserve(
        &mut rest,
        "gemini_thought_signature",
        &part.thought_signature,
    )?;
    preserve(&mut rest, "gemini_part_metadata", &part.part_metadata)?;
    preserve(&mut rest, "gemini_media_resolution", &part.media_resolution)?;
    preserve(&mut rest, "gemini_part_type_metadata", &part.metadata)?;
    Ok(rest)
}

fn preserve<T: serde::Serialize>(
    rest: &mut openai::Rest,
    key: &str,
    value: &Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        rest.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}
