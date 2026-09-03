use gproxy_protocol::openai;

use crate::TransformError;

use super::parts::response_part_to_chat;

fn tool_output_part_to_chat(
    part: openai::ResponseToolOutputContentPart,
) -> Result<openai::ChatContentPart, TransformError> {
    response_part_to_chat(match part {
        openai::ResponseToolOutputContentPart::InputText(part) => {
            openai::ResponseInputContentPart::InputText(part)
        }
        openai::ResponseToolOutputContentPart::InputImage(part) => {
            openai::ResponseInputContentPart::InputImage(part)
        }
        openai::ResponseToolOutputContentPart::InputFile(part) => {
            openai::ResponseInputContentPart::InputFile(part)
        }
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

pub(super) fn output_to_chat(
    output: openai::ResponseOutput,
) -> Result<openai::ChatTextContent, TransformError> {
    Ok(match output {
        openai::ResponseOutput::Text(text) => openai::ChatTextContent::Text(text),
        openai::ResponseOutput::Parts(parts) => openai::ChatTextContent::Parts(
            parts
                .into_iter()
                .map(|part| match tool_output_part_to_chat(part)? {
                    openai::ChatContentPart::Text(part) => {
                        Ok(openai::ChatTextContentPart::Text(part))
                    }
                    openai::ChatContentPart::Unknown(raw) => Err(TransformError::unsupported(
                        "Responses tool output",
                        raw.to_string(),
                    )),
                    unsupported @ (openai::ChatContentPart::ImageUrl(_)
                    | openai::ChatContentPart::File(_)
                    | openai::ChatContentPart::InputAudio(_)) => Err(TransformError::unsupported(
                        "Responses tool output",
                        serde_json::to_string(&unsupported)?,
                    )),
                    #[cfg(not(feature = "exhaustive"))]
                    _ => {
                        return Err(crate::TransformError::unsupported(
                            "protocol enum",
                            "unrecognized external variant",
                        ));
                    }
                })
                .collect::<Result<_, _>>()?,
        ),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}
