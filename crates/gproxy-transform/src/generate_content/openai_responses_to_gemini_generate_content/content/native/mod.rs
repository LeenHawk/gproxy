use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{ContentConverter, messages, wire};

mod calls;
mod results;

pub(super) fn function_call(
    call_id: String,
    name: String,
    arguments: String,
    signature: Option<String>,
) -> Result<gemini::Content, TransformError> {
    Ok(model_content(vec![gemini::Part {
        thought_signature: signature,
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id: Some(call_id),
                name,
                args: Some(wire::json_map(&arguments)?),
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }]))
}

pub(super) fn function_result(
    call_id: String,
    name: String,
    output: openai::ResponseOutput,
) -> Result<gemini::Content, TransformError> {
    let (response, parts) = wire::function_result(output)?;
    Ok(user_content(vec![gemini::Part {
        data: Some(gemini::PartData::FunctionResponse {
            function_response: gemini::FunctionResponse {
                id: Some(call_id),
                name,
                response,
                parts,
                will_continue: None,
                scheduling: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }]))
}

pub(super) fn reasoning(
    summary: Vec<openai::ResponseReasoningSummaryPart>,
    content: Option<Vec<openai::ResponseReasoningTextPart>>,
    encrypted_content: Option<String>,
) -> gemini::Content {
    let text = content
        .into_iter()
        .flatten()
        .map(|part| part.text)
        .chain(summary.into_iter().map(|part| part.text))
        .collect::<String>();
    model_content(vec![messages::text_part(text, true, encrypted_content)])
}

pub(super) fn native_item(
    state: &mut ContentConverter,
    item: openai::TypedResponseItem,
) -> Result<Option<gemini::Content>, TransformError> {
    if let Some(content) = calls::convert(state, &item)? {
        return Ok(Some(content));
    }
    if let Some(content) = results::convert(state, &item)? {
        return Ok(Some(content));
    }
    Ok(None)
}

pub(super) fn model_content(parts: Vec<gemini::Part>) -> gemini::Content {
    content(gemini::ContentRoleKnown::Model, parts)
}

pub(super) fn user_content(parts: Vec<gemini::Part>) -> gemini::Content {
    content(gemini::ContentRoleKnown::User, parts)
}

fn content(role: gemini::ContentRoleKnown, parts: Vec<gemini::Part>) -> gemini::Content {
    gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(role)),
        rest: Default::default(),
    }
}
