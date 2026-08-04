use crate::protocol::{claude, openai};

use super::tools::typed_item_to_claude_message;

pub(super) fn openai_input_to_claude_messages(
    input: Option<openai::ResponseInput>,
    system_role: claude::MessageRole,
) -> Vec<claude::MessageParam> {
    match input {
        Some(openai::ResponseInput::Text(text)) => text_to_claude_message(
            claude::MessageRole::Known(claude::MessageRoleKnown::User),
            text,
        )
        .into_iter()
        .collect(),
        Some(openai::ResponseInput::Items(items)) => items
            .into_iter()
            .filter_map(|item| openai_item_to_claude_message(item, &system_role))
            .collect(),
        None => Vec::new(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn openai_item_to_claude_message(
    item: openai::ResponseItem,
    system_role: &claude::MessageRole,
) -> Option<claude::MessageParam> {
    match item {
        openai::ResponseItem::Message(message) => {
            openai_message_to_claude_message(message, system_role)
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::Compaction {
            encrypted_content,
            ..
        }) => Some(crate::protocol::wire!(claude::MessageParam {
            role: claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            content: claude::MessageContent::Array(vec![claude::ContentBlockParam::Compaction(
                crate::protocol::wire!(claude::CompactionBlock {
                    content: None,
                    encrypted_content: Some(encrypted_content),
                    type_: claude::CompactionBlockType::Compaction,
                    cache_control: None,
                }),
            )]),
            extra: Default::default(),
        })),
        openai::ResponseItem::Typed(typed) => typed_item_to_claude_message(typed),
        _ => None,
    }
}

fn openai_message_to_claude_message(
    message: openai::ResponseMessageItem,
    system_role: &claude::MessageRole,
) -> Option<claude::MessageParam> {
    match message {
        openai::ResponseMessageItem::EasyInput(message) => {
            let role = easy_input_role_to_claude(message.role, system_role);
            let blocks = easy_input_content_to_blocks(message.content);
            blocks_to_claude_message(role, blocks)
        }
        openai::ResponseMessageItem::Input(message) => {
            let role = input_role_to_claude(message.role, system_role);
            let blocks = input_parts_to_blocks(message.content);
            blocks_to_claude_message(role, blocks)
        }
        openai::ResponseMessageItem::Output(message) => {
            let blocks = output_parts_to_blocks(message.content);
            blocks_to_claude_message(
                claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                blocks,
            )
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn text_to_claude_message(role: claude::MessageRole, text: String) -> Option<claude::MessageParam> {
    if text.is_empty() {
        return None;
    }

    Some(crate::protocol::wire!(claude::MessageParam {
        role,
        content: claude::MessageContent::String(text),
        extra: Default::default(),
    }))
}

pub(super) fn blocks_to_claude_message(
    role: claude::MessageRole,
    blocks: Vec<claude::ContentBlockParam>,
) -> Option<claude::MessageParam> {
    if blocks.is_empty() {
        return None;
    }

    Some(crate::protocol::wire!(claude::MessageParam {
        role,
        content: claude::MessageContent::Array(blocks),
        extra: Default::default(),
    }))
}

fn easy_input_role_to_claude(
    role: openai::ResponseEasyInputMessageRole,
    system_role: &claude::MessageRole,
) -> claude::MessageRole {
    match role {
        openai::ResponseEasyInputMessageRole::Assistant => {
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant)
        }
        openai::ResponseEasyInputMessageRole::System
        | openai::ResponseEasyInputMessageRole::Developer => system_role.clone(),
        openai::ResponseEasyInputMessageRole::User => {
            claude::MessageRole::Known(claude::MessageRoleKnown::User)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn input_role_to_claude(
    role: openai::ResponseInputMessageRole,
    system_role: &claude::MessageRole,
) -> claude::MessageRole {
    match role {
        openai::ResponseInputMessageRole::System | openai::ResponseInputMessageRole::Developer => {
            system_role.clone()
        }
        openai::ResponseInputMessageRole::User => {
            claude::MessageRole::Known(claude::MessageRoleKnown::User)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn easy_input_content_to_blocks(
    content: openai::ResponseEasyInputContent,
) -> Vec<claude::ContentBlockParam> {
    match content {
        openai::ResponseEasyInputContent::Text(text) => text_block(text).into_iter().collect(),
        openai::ResponseEasyInputContent::Parts(parts) => input_parts_to_blocks(parts),
        openai::ResponseEasyInputContent::OutputParts(parts) => output_parts_to_blocks(parts),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn input_parts_to_blocks(
    parts: Vec<openai::ResponseInputContentPart>,
) -> Vec<claude::ContentBlockParam> {
    parts
        .into_iter()
        .filter_map(input_part_to_claude_block)
        .collect()
}

fn input_part_to_claude_block(
    part: openai::ResponseInputContentPart,
) -> Option<claude::ContentBlockParam> {
    match part {
        openai::ResponseInputContentPart::InputText { text, .. } => text_block(text),
        openai::ResponseInputContentPart::InputImage {
            file_id, image_url, ..
        } => image_block(file_id, image_url),
        openai::ResponseInputContentPart::InputFile {
            file_data,
            file_id,
            file_url,
            filename,
            ..
        } => document_block(file_id, file_url, file_data, filename),
        openai::ResponseInputContentPart::InputAudio { .. } => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn output_parts_to_blocks(
    parts: Vec<openai::ResponseMessageOutputContentPart>,
) -> Vec<claude::ContentBlockParam> {
    parts
        .into_iter()
        .filter_map(|part| match part {
            openai::ResponseMessageOutputContentPart::OutputText { text, .. } => text_block(text),
            openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
                text_block(refusal)
            }
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        })
        .collect()
}

pub(super) fn text_block(text: String) -> Option<claude::ContentBlockParam> {
    if text.is_empty() {
        return None;
    }

    Some(claude::ContentBlockParam::Text(crate::protocol::wire!(
        claude::TextBlock {
            text,
            type_: claude::TextBlockType::Text,
            cache_control: None,
            citations: None,
            extra: Default::default(),
        }
    )))
}

pub(super) fn image_block(
    file_id: Option<String>,
    image_url: Option<String>,
) -> Option<claude::ContentBlockParam> {
    let source = if let Some(file_id) = file_id {
        claude::ImageSource::File(crate::protocol::wire!(claude::FileImageSource {
            file_id,
            type_: claude::FileSourceType::File,
            extra: Default::default(),
        }))
    } else {
        claude::ImageSource::Url(crate::protocol::wire!(claude::UrlImageSource {
            type_: claude::UrlSourceType::Url,
            url: image_url?,
            extra: Default::default(),
        }))
    };

    Some(claude::ContentBlockParam::Image(crate::protocol::wire!(
        claude::ImageBlock {
            source,
            type_: claude::ImageBlockType::Image,
            cache_control: None,
        }
    )))
}

pub(super) fn document_block(
    file_id: Option<String>,
    file_url: Option<String>,
    file_data: Option<String>,
    filename: Option<String>,
) -> Option<claude::ContentBlockParam> {
    let source = if let Some(file_id) = file_id {
        claude::DocumentSource::File(crate::protocol::wire!(claude::FileDocumentSource {
            file_id,
            type_: claude::FileSourceType::File,
            extra: Default::default(),
        }))
    } else if let Some(file_url) = file_url {
        claude::DocumentSource::Url(crate::protocol::wire!(claude::UrlDocumentSource {
            type_: claude::UrlSourceType::Url,
            url: file_url,
            extra: Default::default(),
        }))
    } else {
        claude::DocumentSource::Text(crate::protocol::wire!(claude::PlainTextSource {
            data: file_data?,
            media_type: claude::PlainTextMediaType::TextPlain,
            type_: claude::TextSourceType::Text,
            extra: Default::default(),
        }))
    };

    Some(claude::ContentBlockParam::Document(crate::protocol::wire!(
        claude::DocumentBlock {
            source,
            type_: claude::DocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: filename,
        }
    )))
}
