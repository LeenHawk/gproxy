use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{config, content, tools};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    _stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let mut input: claude::CreateMessageRequestBody = serde_json::from_slice(&body)?;
    crate::common::claude_message_controls::apply(&mut input.messages, &mut input.output_config);
    let generation_config = config::generation(&input)?;
    let tool_config = tools::choice(input.tool_choice);
    let output = gemini::GenerateContentRequest {
        model: Some(model.to_owned()),
        contents: content::request_messages(input.messages)?,
        tools: {
            let tools = tools::definitions(input.tools)?;
            (!tools.is_empty()).then_some(tools)
        },
        tool_config,
        safety_settings: None,
        system_instruction: content::system(input.system)?,
        generation_config: Some(generation_config),
        cached_content: None,
        service_tier: config::request_tier(input.speed, input.service_tier),
        store: None,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
