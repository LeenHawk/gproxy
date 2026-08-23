use gproxy_protocol::openai;

use crate::TransformError;

pub(super) fn user_text(text: String) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::User(openai::ChatUserMessageParam {
        role: openai::ChatUserRole::User,
        content: openai::ChatContent::Text(text),
        name: None,
        rest: Default::default(),
    })
}

pub(super) fn user_content(
    content: openai::ResponseEasyInputContent,
    rest: openai::Rest,
) -> Result<openai::ChatCompletionMessageParam, TransformError> {
    Ok(openai::ChatCompletionMessageParam::User(
        openai::ChatUserMessageParam {
            role: openai::ChatUserRole::User,
            content: easy_user(content)?,
            name: None,
            rest,
        },
    ))
}

pub(super) fn text_message(
    content: openai::ResponseEasyInputContent,
    role: openai::ResponseEasyInputMessageRole,
    rest: openai::Rest,
) -> Result<openai::ChatCompletionMessageParam, TransformError> {
    let content = easy_text(content)?;
    Ok(match role {
        openai::ResponseEasyInputMessageRole::System => {
            openai::ChatCompletionMessageParam::System(openai::ChatSystemMessageParam {
                role: openai::ChatSystemRole::System,
                content,
                name: None,
                rest,
            })
        }
        _ => openai::ChatCompletionMessageParam::Developer(openai::ChatDeveloperMessageParam {
            role: openai::ChatDeveloperRole::Developer,
            content,
            name: None,
            rest,
        }),
    })
}

pub(super) fn easy_text(
    content: openai::ResponseEasyInputContent,
) -> Result<openai::ChatTextContent, TransformError> {
    match content {
        openai::ResponseEasyInputContent::Text(text) => Ok(openai::ChatTextContent::Text(text)),
        openai::ResponseEasyInputContent::Parts(parts) => Ok(openai::ChatTextContent::Parts(
            parts
                .into_iter()
                .map(|part| match part {
                    openai::ResponseInputContentPart::InputText(part) => {
                        Ok(openai::ChatTextContentPart::Text(openai::ChatTextPart {
                            type_: openai::ChatTextPartType::Text,
                            text: part.text,
                            prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                            rest: part.rest,
                        }))
                    }
                    openai::ResponseInputContentPart::Unknown(raw) => {
                        Ok(openai::ChatTextContentPart::Unknown(raw))
                    }
                    other => Err(TransformError::unsupported(
                        "Responses text content",
                        serde_json::to_string(&other)?,
                    )),
                })
                .collect::<Result<_, _>>()?,
        )),
        openai::ResponseEasyInputContent::Unknown(raw) => Ok(openai::ChatTextContent::Unknown(raw)),
        other => Err(TransformError::unsupported(
            "Responses text content",
            serde_json::to_string(&other)?,
        )),
    }
}

pub(super) fn easy_user(
    content: openai::ResponseEasyInputContent,
) -> Result<openai::ChatContent, TransformError> {
    match content {
        openai::ResponseEasyInputContent::Text(text) => Ok(openai::ChatContent::Text(text)),
        openai::ResponseEasyInputContent::Parts(parts) => Ok(openai::ChatContent::Parts(
            parts
                .into_iter()
                .map(response_part_to_chat)
                .collect::<Result<_, _>>()?,
        )),
        openai::ResponseEasyInputContent::Unknown(raw) => Ok(openai::ChatContent::Unknown(raw)),
        other => Err(TransformError::unsupported(
            "Responses user content",
            serde_json::to_string(&other)?,
        )),
    }
}

pub(super) fn response_part_to_chat(
    part: openai::ResponseInputContentPart,
) -> Result<openai::ChatContentPart, TransformError> {
    Ok(match part {
        openai::ResponseInputContentPart::InputText(part) => {
            openai::ChatContentPart::Text(openai::ChatTextPart {
                type_: openai::ChatTextPartType::Text,
                text: part.text,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::InputImage(part) => {
            openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
                type_: openai::ChatImageUrlPartType::ImageUrl,
                image_url: openai::ImageUrl {
                    url: part.image_url.ok_or_else(|| {
                        TransformError::shape("Responses image", "URL is missing")
                    })?,
                    detail: None,
                    rest: Default::default(),
                },
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::InputFile(part) => {
            openai::ChatContentPart::File(openai::ChatFilePart {
                type_: openai::ChatFilePartType::File,
                file: openai::ChatFileRef {
                    file_data: part.file_data,
                    file_id: part.file_id,
                    filename: part.filename,
                    rest: Default::default(),
                },
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::InputAudio(part) => {
            openai::ChatContentPart::InputAudio(openai::ChatInputAudioPart {
                type_: openai::ChatInputAudioPartType::InputAudio,
                input_audio: openai::InputAudio {
                    data: part.input_audio.data,
                    format: part.input_audio.format,
                    rest: part.input_audio.rest,
                },
                prompt_cache_breakpoint: None,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::Unknown(raw) => openai::ChatContentPart::Unknown(raw),
    })
}

pub(super) fn easy_assistant(
    content: openai::ResponseEasyInputContent,
) -> Result<openai::ChatAssistantContent, TransformError> {
    match content {
        openai::ResponseEasyInputContent::Text(text) => {
            Ok(openai::ChatAssistantContent::Text(text))
        }
        openai::ResponseEasyInputContent::OutputParts(parts) => Ok(output_content(parts)),
        openai::ResponseEasyInputContent::Unknown(raw) => {
            Ok(openai::ChatAssistantContent::Unknown(raw))
        }
        other => Err(TransformError::unsupported(
            "Responses assistant content",
            serde_json::to_string(&other)?,
        )),
    }
}

pub(super) fn output_content(
    parts: Vec<openai::ResponseMessageOutputContentPart>,
) -> openai::ChatAssistantContent {
    openai::ChatAssistantContent::Parts(
        parts
            .into_iter()
            .map(|part| match part {
                openai::ResponseMessageOutputContentPart::OutputText(part) => {
                    openai::ChatAssistantContentPart::Text(openai::ChatTextPart {
                        type_: openai::ChatTextPartType::Text,
                        text: part.text,
                        prompt_cache_breakpoint: None,
                        rest: part.rest,
                    })
                }
                openai::ResponseMessageOutputContentPart::Refusal(part) => {
                    openai::ChatAssistantContentPart::Refusal(openai::ChatRefusalPart {
                        type_: openai::ChatRefusalPartType::Refusal,
                        refusal: part.refusal,
                        prompt_cache_breakpoint: None,
                        rest: part.rest,
                    })
                }
                openai::ResponseMessageOutputContentPart::Unknown(raw) => {
                    openai::ChatAssistantContentPart::Unknown(raw)
                }
            })
            .collect(),
    )
}

pub(super) fn output_to_chat(
    output: openai::ResponseOutput,
) -> Result<openai::ChatTextContent, TransformError> {
    Ok(match output {
        openai::ResponseOutput::Text(text) => openai::ChatTextContent::Text(text),
        openai::ResponseOutput::Parts(parts) => openai::ChatTextContent::Parts(
            parts
                .into_iter()
                .map(|part| match response_part_to_chat(part)? {
                    openai::ChatContentPart::Text(part) => {
                        Ok(openai::ChatTextContentPart::Text(part))
                    }
                    openai::ChatContentPart::Unknown(raw) => {
                        Ok(openai::ChatTextContentPart::Unknown(raw))
                    }
                    other => Err(TransformError::unsupported(
                        "Responses tool output",
                        serde_json::to_string(&other)?,
                    )),
                })
                .collect::<Result<_, _>>()?,
        ),
        openai::ResponseOutput::Unknown(raw) => openai::ChatTextContent::Unknown(raw),
    })
}
