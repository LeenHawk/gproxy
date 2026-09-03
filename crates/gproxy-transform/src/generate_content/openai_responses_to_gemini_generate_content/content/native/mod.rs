use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{ContentConverter, messages, wire};

mod calls;
mod results;

pub(super) fn function_call(
    call_id: String,
    name: String,
    arguments: String,
    rest: &mut openai::Rest,
) -> Result<gemini::Content, TransformError> {
    let signature = rest
        .remove("thought_signature")
        .or_else(|| rest.remove("thoughtSignature"))
        .and_then(|value| value.as_str().map(str::to_owned));
    Ok(model_content(
        vec![gemini::Part {
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
        }],
        Default::default(),
    ))
}

pub(super) fn function_result(
    call_id: String,
    name: String,
    output: openai::ResponseOutput,
    _rest: openai::Rest,
) -> Result<gemini::Content, TransformError> {
    let (response, parts) = wire::function_result(output)?;
    Ok(user_content(
        vec![gemini::Part {
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
        }],
        Default::default(),
    ))
}

pub(super) fn reasoning(
    id: Option<String>,
    summary: Vec<openai::ResponseReasoningSummaryPart>,
    content: Option<Vec<openai::ResponseReasoningTextPart>>,
    encrypted_content: Option<String>,
    rest: openai::Rest,
) -> gemini::Content {
    let text = content
        .into_iter()
        .flatten()
        .map(|part| part.text)
        .chain(summary.into_iter().map(|part| part.text))
        .collect::<String>();
    let mut part = messages::text_part(text, true, encrypted_content);
    part.rest = wire::openai_item_rest(rest, id);
    model_content(vec![part], Default::default())
}

pub(super) fn native_item(
    state: &mut ContentConverter,
    item: openai::TypedResponseItem,
) -> Result<gemini::Content, TransformError> {
    if let Some(content) = calls::convert(state, &item)? {
        return Ok(content);
    }
    if let Some(content) = results::convert(state, &item)? {
        return Ok(content);
    }
    Ok(model_content(
        vec![gemini::Part {
            data: Some(gemini::PartData::Raw(serde_json::to_value(item)?)),
            ..Default::default()
        }],
        Default::default(),
    ))
}

pub(super) fn model_content(parts: Vec<gemini::Part>, rest: gemini::JsonMap) -> gemini::Content {
    content(gemini::ContentRoleKnown::Model, parts, rest)
}

pub(super) fn user_content(parts: Vec<gemini::Part>, rest: gemini::JsonMap) -> gemini::Content {
    content(gemini::ContentRoleKnown::User, parts, rest)
}

fn content(
    role: gemini::ContentRoleKnown,
    parts: Vec<gemini::Part>,
    rest: gemini::JsonMap,
) -> gemini::Content {
    gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(role)),
        rest,
    }
}
