use super::{invalid, tools};
use crate::channel::ChannelError;
use gproxy_protocol::{connection::Bytes, wire::openai::responses::*};

pub(super) fn request(body: &Bytes) -> Result<(Bytes, tools::Aliases), ChannelError> {
    let mut request: GenerateContentRequestBody = serde_json::from_slice(body).map_err(invalid)?;
    request.stream = Some(Some(true));
    request.store = Some(Some(false));
    request.max_output_tokens = None;
    request.metadata = None;
    request.prompt_cache_options = None;
    request.temperature = None;
    request.top_p = None;
    request.top_logprobs = None;
    request.safety_identifier = None;
    request.truncation = None;
    if let Some(Input::Text(text)) = request
        .input
        .take_if(|input| matches!(input, Input::Text(_)))
    {
        request.input = Some(Input::Items(vec![InputItem::Easy(EasyInputMessage {
            type_: Some(MessageType::Message),
            role: MessageRole::User,
            content: MessageContent::Text(text),
            phase: None,
            rest: Default::default(),
        })]));
    }
    let mut aliases = tools::normalize_definitions(&mut request.tools, &mut request.tool_choice)?;
    if let Some(Input::Items(items)) = request.input.as_mut() {
        let mut retained = Vec::with_capacity(items.len());
        for mut item in std::mem::take(items) {
            if let Some(text) = system_text(&item) {
                if !text.is_empty() {
                    let instructions = request
                        .instructions
                        .get_or_insert(None)
                        .get_or_insert_default();
                    if !instructions.is_empty() {
                        instructions.push('\n');
                    }
                    instructions.push_str(&text);
                }
            } else {
                if let InputItem::Reasoning(reasoning) = &mut item {
                    reasoning.status = None;
                }
                retained.push(item);
            }
        }
        tools::normalize_history(&mut retained, &mut aliases)?;
        *items = retained;
    }
    let bytes = serde_json::to_vec(&request).map_err(invalid)?;
    Ok((Bytes::from(bytes), aliases))
}

fn system_text(item: &InputItem) -> Option<String> {
    match item {
        InputItem::Message(message) if message.role == InputMessageRole::System => {
            Some(parts_text(&message.content))
        }
        InputItem::Easy(message) if message.role == MessageRole::System => {
            Some(match &message.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Parts(parts) => parts_text(parts),
            })
        }
        _ => None,
    }
}

fn parts_text(parts: &[InputContent]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            InputContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
