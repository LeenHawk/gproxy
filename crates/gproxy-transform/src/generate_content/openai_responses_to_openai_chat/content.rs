use gproxy_protocol::openai;

use crate::TransformError;

mod parts;
mod tool_output;

use parts::response_part_to_chat;

pub(super) fn output_to_chat(
    output: openai::ResponseOutput,
) -> Result<openai::ChatTextContent, TransformError> {
    tool_output::output_to_chat(output)
}

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
        openai::ResponseEasyInputMessageRole::Developer
        | openai::ResponseEasyInputMessageRole::User
        | openai::ResponseEasyInputMessageRole::Assistant => {
            openai::ChatCompletionMessageParam::Developer(openai::ChatDeveloperMessageParam {
                role: openai::ChatDeveloperRole::Developer,
                content,
                name: None,
                rest,
            })
        }
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
                    unsupported @ (openai::ResponseInputContentPart::InputImage(_)
                    | openai::ResponseInputContentPart::InputFile(_)
                    | openai::ResponseInputContentPart::InputAudio(_)) => {
                        Err(TransformError::unsupported(
                            "Responses text content",
                            serde_json::to_string(&unsupported)?,
                        ))
                    }
                })
                .collect::<Result<_, _>>()?,
        )),
        openai::ResponseEasyInputContent::Unknown(raw) => Ok(openai::ChatTextContent::Unknown(raw)),
        openai::ResponseEasyInputContent::OutputParts(parts) => {
            Ok(openai::ChatTextContent::Parts(output_text_parts(parts)))
        }
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
        openai::ResponseEasyInputContent::OutputParts(parts) => Ok(openai::ChatContent::Parts(
            output_text_parts(parts)
                .into_iter()
                .filter_map(|part| match part {
                    openai::ChatTextContentPart::Text(part) => {
                        Some(openai::ChatContentPart::Text(part))
                    }
                    openai::ChatTextContentPart::Unknown(_) => None,
                })
                .collect(),
        )),
    }
}

fn output_text_parts(
    parts: Vec<openai::ResponseMessageOutputContentPart>,
) -> Vec<openai::ChatTextContentPart> {
    parts
        .into_iter()
        .filter_map(|part| match part {
            openai::ResponseMessageOutputContentPart::OutputText(part) => {
                Some(openai::ChatTextContentPart::Text(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: part.text,
                    prompt_cache_breakpoint: None,
                    rest: part.rest,
                }))
            }
            openai::ResponseMessageOutputContentPart::Refusal(part) => {
                Some(openai::ChatTextContentPart::Text(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: part.refusal,
                    prompt_cache_breakpoint: None,
                    rest: part.rest,
                }))
            }
            openai::ResponseMessageOutputContentPart::Unknown(_) => None,
        })
        .collect()
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
        unsupported @ openai::ResponseEasyInputContent::Parts(_) => {
            Err(TransformError::unsupported(
                "Responses assistant content",
                serde_json::to_string(&unsupported)?,
            ))
        }
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
