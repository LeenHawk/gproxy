use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn transform(body: bytes::Bytes, model: &str) -> Result<bytes::Bytes, TransformError> {
    let input: openai::CompactResponseRequestBody = serde_json::from_slice(&body)?;
    let output = transform_typed(input, model)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::CompactResponseRequestBody,
    model: &str,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let instructions = input.instructions.clone();
    let request = super::super::other::compact_to_responses_typed(input, model);
    let mut output =
        crate::generate_content::openai_responses_to_claude_messages::request::transform_typed(
            request, model, false,
        )?;
    output.context_management = Some(crate::wire!(claude::ContextManagementConfig {
        edits: Some(vec![claude::ContextEdit::Known(
            claude::KnownContextEdit::Compact {
                instructions,
                pause_after_compaction: Some(true),
                trigger: None,
                rest: Default::default(),
            },
        )]),
        rest: Default::default(),
    }));
    Ok(output)
}
