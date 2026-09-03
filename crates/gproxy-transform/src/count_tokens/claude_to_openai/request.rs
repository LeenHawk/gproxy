use gproxy_protocol::claude;

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CountTokensRequestBody = serde_json::from_slice(&body)?;
    let output = transform_typed(input, model)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    mut input: claude::CountTokensRequestBody,
    model: &str,
) -> Result<gproxy_protocol::openai::ResponseInputTokensRequest, TransformError> {
    crate::common::claude_message_controls::apply(&mut input.messages, &mut input.output_config);
    let text = messages_text(std::mem::take(&mut input.messages));
    input.cache_control = None;
    input.context_management = None;
    input.mcp_servers = None;
    let mut output =
        crate::generate_content::claude_messages_to_openai_responses::request::count_tokens(
            input, model,
        )?;
    output.input = (!text.is_empty()).then_some(gproxy_protocol::openai::ResponseInput::Text(text));
    Ok(output)
}

fn messages_text(messages: Vec<claude::MessageParam>) -> String {
    messages
        .into_iter()
        .map(|message| {
            if let claude::StringOrArray::String(text) = &message.content {
                return text.clone();
            }
            if let claude::StringOrArray::Array(blocks) = message.content {
                return blocks
                    .into_iter()
                    .filter_map(|block| {
                        let claude::ContentBlockParam::Text(block) = block else {
                            return None;
                        };
                        Some(block.text)
                    })
                    .collect();
            }
            String::new()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
