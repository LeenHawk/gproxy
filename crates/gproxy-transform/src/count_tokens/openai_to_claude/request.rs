use gproxy_protocol::openai;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let mut input: openai::ResponseInputTokensRequest = serde_json::from_slice(&body)?;
    let text = input_text(input.input.take());
    let mut output =
        crate::generate_content::openai_responses_to_claude_messages::request::count_tokens(
            input, model,
        )?;
    output.messages = if text.is_empty() {
        Vec::new()
    } else {
        vec![gproxy_protocol::claude::MessageParam {
            role: gproxy_protocol::claude::MessageRole::Known(
                gproxy_protocol::claude::MessageRoleKnown::User,
            ),
            content: gproxy_protocol::claude::StringOrArray::String(text),
            clear_at: None,
            output_config: None,
            rest: Default::default(),
        }]
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn input_text(input: Option<openai::ResponseInput>) -> String {
    match input {
        None => String::new(),
        Some(openai::ResponseInput::Text(text)) => text,
        Some(openai::ResponseInput::Items(items)) => items
            .into_iter()
            .map(item_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Some(openai::ResponseInput::Unknown(_)) => String::new(),
    }
}

fn item_text(item: openai::ResponseItem) -> String {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(message)) => {
            easy_text(message.content)
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Input(message)) => {
            input_parts_text(message.content)
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => message
            .content
            .into_iter()
            .map(|part| match part {
                openai::ResponseMessageOutputContentPart::OutputText(part) => part.text,
                openai::ResponseMessageOutputContentPart::Refusal(part) => part.refusal,
                openai::ResponseMessageOutputContentPart::Unknown(_) => String::new(),
            })
            .collect(),
        openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(_))
        | openai::ResponseItem::Typed(_)
        | openai::ResponseItem::Unknown(_) => String::new(),
    }
}

fn easy_text(content: openai::ResponseEasyInputContent) -> String {
    match content {
        openai::ResponseEasyInputContent::Text(text) => text,
        openai::ResponseEasyInputContent::Parts(parts) => input_parts_text(parts),
        openai::ResponseEasyInputContent::OutputParts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ResponseMessageOutputContentPart::OutputText(part) => part.text,
                openai::ResponseMessageOutputContentPart::Refusal(part) => part.refusal,
                openai::ResponseMessageOutputContentPart::Unknown(_) => String::new(),
            })
            .collect(),
        openai::ResponseEasyInputContent::Unknown(_) => String::new(),
    }
}

fn input_parts_text(parts: Vec<openai::ResponseInputContentPart>) -> String {
    parts
        .into_iter()
        .filter_map(|part| match part {
            openai::ResponseInputContentPart::InputText(part) => Some(part.text),
            openai::ResponseInputContentPart::InputImage(_)
            | openai::ResponseInputContentPart::InputFile(_)
            | openai::ResponseInputContentPart::InputAudio(_) => None,
        })
        .collect()
}
