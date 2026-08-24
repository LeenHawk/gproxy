use gproxy_protocol::openai;

use crate::TransformError;

use super::messages::tool_output_part;

pub(super) fn text_content(
    content: openai::ChatTextContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatTextContent::Text(text) => openai::ResponseEasyInputContent::Text(text),
        openai::ChatTextContent::Parts(parts) => openai::ResponseEasyInputContent::Parts(
            parts
                .into_iter()
                .map(|part| match part {
                    openai::ChatTextContentPart::Text(part) => Ok(
                        openai::ResponseInputContentPart::InputText(openai::ResponseInputText {
                            text: part.text,
                            prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                            rest: part.rest,
                        }),
                    ),
                    openai::ChatTextContentPart::Unknown(raw) => Err(TransformError::unsupported(
                        "Chat text content",
                        raw.to_string(),
                    )),
                })
                .collect::<Result<_, _>>()?,
        ),
        openai::ChatTextContent::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat text content",
                raw.to_string(),
            ));
        }
    })
}

pub(super) fn user_content(
    content: openai::ChatContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatContent::Text(text) => openai::ResponseEasyInputContent::Text(text),
        openai::ChatContent::Parts(parts) => openai::ResponseEasyInputContent::Parts(
            parts
                .into_iter()
                .map(chat_part_to_response)
                .collect::<Result<_, _>>()?,
        ),
        openai::ChatContent::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat user content",
                raw.to_string(),
            ));
        }
    })
}

pub(super) fn assistant_content(
    content: openai::ChatAssistantContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatAssistantContent::Text(text) => {
            openai::ResponseEasyInputContent::OutputParts(vec![
                openai::ResponseMessageOutputContentPart::OutputText(openai::ResponseOutputText {
                    type_: openai::ResponseOutputTextType::OutputText,
                    annotations: Vec::new(),
                    logprobs: None,
                    text,
                    rest: Default::default(),
                }),
            ])
        }
        openai::ChatAssistantContent::Parts(parts) => {
            openai::ResponseEasyInputContent::OutputParts(
                parts
                    .into_iter()
                    .map(|part| match part {
                        openai::ChatAssistantContentPart::Text(part) => {
                            Ok(openai::ResponseMessageOutputContentPart::OutputText(
                                openai::ResponseOutputText {
                                    type_: openai::ResponseOutputTextType::OutputText,
                                    annotations: Vec::new(),
                                    logprobs: None,
                                    text: part.text,
                                    rest: part.rest,
                                },
                            ))
                        }
                        openai::ChatAssistantContentPart::Refusal(part) => {
                            Ok(openai::ResponseMessageOutputContentPart::Refusal(
                                openai::ResponseRefusal {
                                    type_: openai::ResponseRefusalType::Refusal,
                                    refusal: part.refusal,
                                    rest: part.rest,
                                },
                            ))
                        }
                        openai::ChatAssistantContentPart::Unknown(raw) => Err(
                            TransformError::unsupported("Chat assistant content", raw.to_string()),
                        ),
                    })
                    .collect::<Result<_, _>>()?,
            )
        }
        openai::ChatAssistantContent::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat assistant content",
                raw.to_string(),
            ));
        }
    })
}

fn chat_part_to_response(
    part: openai::ChatContentPart,
) -> Result<openai::ResponseInputContentPart, TransformError> {
    Ok(match part {
        openai::ChatContentPart::Text(part) => {
            openai::ResponseInputContentPart::InputText(openai::ResponseInputText {
                text: part.text,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::ImageUrl(part) => {
            openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                detail: None,
                file_id: None,
                image_url: Some(part.image_url.url),
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::File(part) => {
            openai::ResponseInputContentPart::InputFile(openai::ResponseInputFile {
                detail: None,
                file_data: part.file.file_data,
                file_id: part.file.file_id,
                file_url: None,
                filename: part.file.filename,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::InputAudio(part) => {
            openai::ResponseInputContentPart::InputAudio(openai::ResponseInputAudio {
                input_audio: openai::InputAudioContent {
                    data: part.input_audio.data,
                    format: part.input_audio.format,
                    rest: part.input_audio.rest,
                },
                rest: part.rest,
            })
        }
        openai::ChatContentPart::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat content part",
                raw.to_string(),
            ));
        }
    })
}

pub(super) fn text_output(
    content: openai::ChatTextContent,
) -> Result<openai::ResponseOutput, TransformError> {
    Ok(match text_content(content)? {
        openai::ResponseEasyInputContent::Text(text) => openai::ResponseOutput::Text(text),
        openai::ResponseEasyInputContent::Parts(parts) => openai::ResponseOutput::Parts(
            parts
                .into_iter()
                .map(tool_output_part)
                .collect::<Result<_, _>>()?,
        ),
        openai::ResponseEasyInputContent::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat tool output",
                raw.to_string(),
            ));
        }
        openai::ResponseEasyInputContent::OutputParts(_) => {
            return Err(TransformError::shape(
                "Chat tool output",
                "unexpected assistant output parts",
            ));
        }
    })
}
