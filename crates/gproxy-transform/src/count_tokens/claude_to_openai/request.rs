use gproxy_protocol::claude;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let mut input: claude::CountTokensRequestBody = serde_json::from_slice(&body)?;
    let text = messages_text(std::mem::take(&mut input.messages));
    input.cache_control = None;
    input.context_management = None;
    input.mcp_servers = None;
    let mut output =
        crate::generate_content::claude_messages_to_openai_responses::request::count_tokens(
            input, model,
        )?;
    output.input = (!text.is_empty()).then_some(gproxy_protocol::openai::ResponseInput::Text(text));
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn messages_text(messages: Vec<claude::MessageParam>) -> String {
    messages
        .into_iter()
        .map(|message| match message.content {
            claude::StringOrArray::String(text) => text,
            claude::StringOrArray::Array(blocks) => blocks
                .into_iter()
                .filter_map(|block| match block {
                    claude::ContentBlockParam::Text(block) => Some(block.text),
                    _ => None,
                })
                .collect(),
            claude::StringOrArray::Raw(_) => String::new(),
            _future => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
