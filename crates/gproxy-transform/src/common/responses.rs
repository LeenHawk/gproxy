use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::content;

pub(crate) fn input_to_claude(
    parts: Vec<openai::ResponseInputContentPart>,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    let mut chat = Vec::new();
    for part in parts {
        chat.push(match part {
            openai::ResponseInputContentPart::InputText(part) => {
                openai::ChatContentPart::Text(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: part.text,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                })
            }
            openai::ResponseInputContentPart::InputImage(part) => {
                if let Some(file_id) = part.file_id {
                    openai::ChatContentPart::File(openai::ChatFilePart {
                        type_: openai::ChatFilePartType::File,
                        file: openai::ChatFileRef {
                            file_data: None,
                            file_id: Some(file_id),
                            filename: None,
                            rest: Default::default(),
                        },
                        prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                        rest: part.rest,
                    })
                } else {
                    openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
                        type_: openai::ChatImageUrlPartType::ImageUrl,
                        image_url: openai::ImageUrl {
                            url: part.image_url.ok_or_else(|| {
                                TransformError::shape("Responses image", "source is missing")
                            })?,
                            detail: None,
                            rest: Default::default(),
                        },
                        prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                        rest: part.rest,
                    })
                }
            }
            openai::ResponseInputContentPart::InputFile(part) => {
                if part.file_url.is_some() {
                    return Err(TransformError::unsupported("Responses file", "file_url"));
                }
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
            openai::ResponseInputContentPart::InputAudio(_) => {
                return Err(TransformError::unsupported(
                    "Responses content",
                    "input_audio",
                ));
            }
        });
    }
    content::chat_user_blocks(openai::ChatContent::Parts(chat))
}

pub(crate) fn claude_to_input(
    blocks: Vec<claude::ContentBlockParam>,
) -> Result<Vec<openai::ResponseInputContentPart>, TransformError> {
    content::claude_user_parts(blocks)?
        .into_iter()
        .map(|part| match part {
            openai::ChatContentPart::Text(part) => Ok(openai::ResponseInputContentPart::InputText(
                openai::ResponseInputText {
                    text: part.text,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                },
            )),
            openai::ChatContentPart::ImageUrl(part) => Ok(
                openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                    detail: None,
                    file_id: None,
                    image_url: Some(part.image_url.url),
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                }),
            ),
            openai::ChatContentPart::File(part) => Ok(openai::ResponseInputContentPart::InputFile(
                openai::ResponseInputFile {
                    detail: None,
                    file_data: part.file.file_data,
                    file_id: part.file.file_id,
                    file_url: None,
                    filename: part.file.filename,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                },
            )),
            openai::ChatContentPart::Unknown(raw) => Err(TransformError::unsupported(
                "Claude content",
                serde_json::to_string(&raw)?,
            )),
            openai::ChatContentPart::InputAudio(_) => {
                Err(TransformError::unsupported("Claude content", "audio"))
            }
        })
        .collect()
}

pub(crate) fn output_to_claude(
    parts: Vec<openai::ResponseMessageOutputContentPart>,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    let chat = parts
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
        .collect();
    content::chat_assistant_blocks(openai::ChatAssistantContent::Parts(chat))
}
