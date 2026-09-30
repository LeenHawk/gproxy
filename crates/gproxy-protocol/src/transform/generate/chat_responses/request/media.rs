use crate::{
    transform::TransformError,
    wire::openai::{chat, responses},
};

pub(super) fn user_message(
    content: chat::content::UserContent,
) -> Result<responses::input::InputItem, TransformError> {
    let content = match content {
        chat::content::UserContentParts::Text(text) => responses::input::MessageContent::Text(text),
        chat::content::UserContentParts::Parts(parts) => responses::input::MessageContent::Parts(
            parts
                .into_iter()
                .map(|part| match part {
                    chat::content::UserContentPart::Text(part) => Ok(
                        responses::input::InputContent::Text(responses::input::ResponseInputText {
                            type_: responses::input::ResponseInputTextType::ResponseInputText,
                            text: part.text,
                            prompt_cache_breakpoint: part
                                .prompt_cache_breakpoint
                                .map(to_responses_cache),
                            rest: Default::default(),
                        }),
                    ),
                    chat::content::UserContentPart::Image(part) => {
                        Ok(responses::input::InputContent::Image(
                            responses::input::ResponseInputImage {
                                type_: responses::input::ResponseInputImageType::ResponseInputImage,
                                detail: match part
                                    .image_url
                                    .detail
                                    .unwrap_or(chat::content::ImageDetail::Auto)
                                {
                                    chat::content::ImageDetail::Auto => {
                                        responses::input::ImageDetail::Auto
                                    }
                                    chat::content::ImageDetail::Low => {
                                        responses::input::ImageDetail::Low
                                    }
                                    chat::content::ImageDetail::High => {
                                        responses::input::ImageDetail::High
                                    }
                                },
                                file_id: None,
                                image_url: Some(Some(part.image_url.url)),
                                prompt_cache_breakpoint: part
                                    .prompt_cache_breakpoint
                                    .map(to_responses_cache),
                                rest: Default::default(),
                            },
                        ))
                    }
                    chat::content::UserContentPart::File(part) => Ok(
                        responses::input::InputContent::File(responses::input::ResponseInputFile {
                            type_: responses::input::ResponseInputFileType::ResponseInputFile,
                            detail: None,
                            file_data: part.file.file_data,
                            file_id: part.file.file_id.map(Some),
                            file_url: None,
                            filename: part.file.filename,
                            prompt_cache_breakpoint: part
                                .prompt_cache_breakpoint
                                .map(to_responses_cache),
                            rest: Default::default(),
                        }),
                    ),
                    chat::content::UserContentPart::InputAudio(_) => {
                        Err(TransformError::unsupported(
                            "messages.content",
                            "Chat audio has no Responses input audio equivalent",
                        ))
                    }
                })
                .filter_map(|value| crate::transform::optional(value).transpose())
                .collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(responses::input::InputItem::Easy(
        responses::input::EasyInputMessage {
            agent: None,
            content,
            role: responses::input::MessageRole::User,
            phase: None,
            type_: None,
            rest: Default::default(),
        },
    ))
}

pub(super) fn to_chat(
    content: responses::input::MessageContent,
    report: &mut crate::transform::Report,
) -> Result<chat::UserContent, TransformError> {
    use responses::input as r;
    Ok(match content {
        r::MessageContent::Text(text) => chat::UserContent::Text(text),
        r::MessageContent::Parts(parts) => chat::UserContent::Parts(
            parts
                .into_iter()
                .map(|part| {
                    Ok(match part {
                        r::InputContent::Text(part) => {
                            let mut target =
                                chat::TextPart::builder(chat::TextPartType::Text, part.text)
                                    .build();
                            target.prompt_cache_breakpoint =
                                part.prompt_cache_breakpoint.map(to_chat_cache);
                            chat::UserContentPart::Text(target)
                        }
                        r::InputContent::Image(part) => {
                            if part.file_id.flatten().is_some() {
                                return Err(TransformError::missing_metadata(
                                    "input_image.file_id resource URL",
                                ));
                            }
                            let url = part.image_url.flatten().ok_or_else(|| {
                                TransformError::missing_metadata("input_image.image_url")
                            })?;
                            let mut image = chat::ImageUrl::builder(url).build();
                            image.detail = Some(match part.detail {
                                r::ImageDetail::Auto => chat::ImageDetail::Auto,
                                r::ImageDetail::Low => chat::ImageDetail::Low,
                                r::ImageDetail::High => chat::ImageDetail::High,
                                r::ImageDetail::Original => {
                                    return Err(TransformError::unsupported(
                                        "input_image.detail",
                                        "Chat has no original-resolution mode",
                                    ));
                                }
                            });
                            let mut target =
                                chat::ImagePart::builder(chat::ImagePartType::ImageUrl, image)
                                    .build();
                            target.prompt_cache_breakpoint =
                                part.prompt_cache_breakpoint.map(to_chat_cache);
                            chat::UserContentPart::Image(target)
                        }
                        r::InputContent::File(part) => {
                            if part.file_url.is_some() {
                                return Err(TransformError::missing_metadata(
                                    "input_file.file_url resource data",
                                ));
                            }
                            if part.detail.is_some() {
                                report.omitted(
                                    "input_file.detail",
                                    "Chat file parts have no detail selector",
                                );
                            }
                            let mut file = chat::FileRef::builder().build();
                            file.file_data = part.file_data;
                            file.file_id = part.file_id.flatten();
                            file.filename = part.filename;
                            let mut target =
                                chat::FilePart::builder(chat::FilePartType::File, file).build();
                            target.prompt_cache_breakpoint =
                                part.prompt_cache_breakpoint.map(to_chat_cache);
                            chat::UserContentPart::File(target)
                        }
                    })
                })
                .collect::<Result<Vec<_>, TransformError>>()?,
        ),
    })
}

fn to_responses_cache(
    value: chat::PromptCacheBreakpoint,
) -> responses::input::PromptCacheBreakpoint {
    match value.mode {
        chat::PromptCacheMode::Explicit => responses::input::PromptCacheBreakpoint::builder(
            responses::input::PromptCacheMode::Explicit,
        )
        .build(),
    }
}

fn to_chat_cache(value: responses::input::PromptCacheBreakpoint) -> chat::PromptCacheBreakpoint {
    match value.mode {
        responses::input::PromptCacheMode::Explicit => {
            chat::PromptCacheBreakpoint::builder(chat::PromptCacheMode::Explicit).build()
        }
    }
}
