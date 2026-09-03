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
                openai::ChatContentPart::Text(crate::wire!(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: part.text,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: Default::default(),
                }))
            }
            openai::ResponseInputContentPart::InputImage(part) => {
                if let Some(file_id) = part.file_id {
                    openai::ChatContentPart::File(openai::ChatFilePart {
                        type_: openai::ChatFilePartType::File,
                        file: crate::wire!(openai::ChatFileRef {
                            file_data: None,
                            file_id: Some(file_id),
                            filename: None,
                            rest: Default::default(),
                        }),
                        prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                        rest: Default::default(),
                    })
                } else {
                    openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
                        type_: openai::ChatImageUrlPartType::ImageUrl,
                        image_url: crate::wire!(openai::ImageUrl {
                            url: part.image_url.ok_or_else(|| {
                                TransformError::shape("Responses image", "source is missing")
                            })?,
                            detail: None,
                            rest: Default::default(),
                        }),
                        prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                        rest: Default::default(),
                    })
                }
            }
            openai::ResponseInputContentPart::InputFile(part) => {
                if part.file_url.is_some() {
                    return Err(TransformError::unsupported("Responses file", "file_url"));
                }
                openai::ChatContentPart::File(openai::ChatFilePart {
                    type_: openai::ChatFilePartType::File,
                    file: crate::wire!(openai::ChatFileRef {
                        file_data: part.file_data,
                        file_id: part.file_id,
                        filename: part.filename,
                        rest: Default::default(),
                    }),
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: Default::default(),
                })
            }
            openai::ResponseInputContentPart::InputAudio(_) => {
                return Err(TransformError::unsupported(
                    "Responses content",
                    "input_audio",
                ));
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
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
                crate::wire!(openai::ResponseInputText {
                    text: part.text,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: Default::default(),
                }),
            )),
            openai::ChatContentPart::ImageUrl(part) => {
                Ok(openai::ResponseInputContentPart::InputImage(crate::wire!(
                    openai::ResponseInputImage {
                        detail: None,
                        file_id: None,
                        image_url: Some(part.image_url.url),
                        prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                        rest: Default::default(),
                    }
                )))
            }
            openai::ChatContentPart::File(part) => Ok(openai::ResponseInputContentPart::InputFile(
                crate::wire!(openai::ResponseInputFile {
                    detail: None,
                    file_data: part.file.file_data,
                    file_id: part.file.file_id,
                    file_url: None,
                    filename: part.file.filename,
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: Default::default(),
                }),
            )),
            openai::ChatContentPart::Unknown(raw) => Err(TransformError::unsupported(
                "Claude content",
                serde_json::to_string(&raw)?,
            )),
            openai::ChatContentPart::InputAudio(_) => {
                Err(TransformError::unsupported("Claude content", "audio"))
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        })
        .collect()
}

pub(crate) fn output_to_claude(
    parts: Vec<openai::ResponseMessageOutputContentPart>,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    let chat = parts
        .into_iter()
        .filter_map(|part| match part {
            openai::ResponseMessageOutputContentPart::OutputText(part) => Some(
                openai::ChatAssistantContentPart::Text(crate::wire!(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: part.text,
                    prompt_cache_breakpoint: None,
                    rest: Default::default(),
                })),
            ),
            openai::ResponseMessageOutputContentPart::Refusal(part) => Some(
                openai::ChatAssistantContentPart::Refusal(crate::wire!(openai::ChatRefusalPart {
                    type_: openai::ChatRefusalPartType::Refusal,
                    refusal: part.refusal,
                    prompt_cache_breakpoint: None,
                    rest: Default::default(),
                })),
            ),
            openai::ResponseMessageOutputContentPart::Unknown(_) => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => None,
        })
        .collect();
    content::chat_assistant_blocks(openai::ChatAssistantContent::Parts(chat))
}
