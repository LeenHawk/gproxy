use bytes::Bytes;
use gproxy_protocol::{WireFamily, claude, gemini, openai};

use crate::TransformError;

pub(crate) fn request(
    source: WireFamily,
    target: WireFamily,
    body: Bytes,
    model: &str,
) -> Result<Bytes, TransformError> {
    match (source, target) {
        (WireFamily::OpenAi, WireFamily::Gemini) => {
            encode(&openai_to_gemini(serde_json::from_slice(&body)?, model)?)
        }
        (WireFamily::Claude, WireFamily::Gemini) => {
            encode(&claude_to_gemini(serde_json::from_slice(&body)?, model)?)
        }
        (WireFamily::Gemini, WireFamily::OpenAi) => {
            encode(&gemini_to_openai(serde_json::from_slice(&body)?, model)?)
        }
        (WireFamily::Gemini, WireFamily::Claude) => {
            encode(&gemini_to_claude(serde_json::from_slice(&body)?, model)?)
        }
        _ => Err(TransformError::shape(
            "count tokens",
            "unsupported Gemini pair",
        )),
    }
}

pub(crate) fn response(
    source: WireFamily,
    target: WireFamily,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    match (source, target) {
        (WireFamily::OpenAi, WireFamily::Gemini) => {
            let input: gemini::CountTokensResponse = serde_json::from_slice(&body)?;
            encode(&gemini_response_to_openai(input))
        }
        (WireFamily::Gemini, WireFamily::OpenAi) => {
            let input: openai::ResponseInputTokensResponse = serde_json::from_slice(&body)?;
            encode(&openai_response_to_gemini(input))
        }
        (WireFamily::Claude, WireFamily::Gemini) => {
            let input: gemini::CountTokensResponse = serde_json::from_slice(&body)?;
            encode(&gemini_response_to_claude(input))
        }
        (WireFamily::Gemini, WireFamily::Claude) => {
            let input: claude::CountTokensResponseBody = serde_json::from_slice(&body)?;
            encode(&claude_response_to_gemini(input))
        }
        _ => Err(TransformError::shape(
            "count tokens",
            "unsupported Gemini pair",
        )),
    }
}

pub(crate) fn openai_to_gemini(
    input: openai::ResponseInputTokensRequest,
    model: &str,
) -> Result<gemini::CountTokensRequest, TransformError> {
    let response: openai::ResponseCreateRequest =
        serde_json::from_value(serde_json::to_value(input)?)?;
    let transformed =
        crate::generate_content::openai_responses_to_gemini_generate_content::request::transform_typed(
            response,
            model,
            false,
        )?;
    wrap_gemini(transformed, model)
}

pub(crate) fn claude_to_gemini(
    input: claude::CountTokensRequestBody,
    model: &str,
) -> Result<gemini::CountTokensRequest, TransformError> {
    let mut value = serde_json::to_value(input)?;
    value
        .as_object_mut()
        .expect("typed Claude request is an object")
        .insert("max_tokens".into(), serde_json::json!(1));
    let transformed =
        crate::generate_content::claude_messages_to_gemini_generate_content::request::transform_typed(
            serde_json::from_value(value)?,
            model,
            false,
        )?;
    wrap_gemini(transformed, model)
}

pub(crate) fn gemini_to_openai(
    input: gemini::CountTokensRequest,
    model: &str,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    let request = gemini_generation(input, model);
    let transformed =
        crate::generate_content::gemini_generate_content_to_openai_responses::request::transform_typed(
            request,
            model,
            false,
        )?;
    Ok(serde_json::from_value(serde_json::to_value(transformed)?)?)
}

pub(crate) fn gemini_to_claude(
    input: gemini::CountTokensRequest,
    model: &str,
) -> Result<claude::CountTokensRequestBody, TransformError> {
    let request = gemini_generation(input, model);
    let transformed =
        crate::generate_content::gemini_generate_content_to_claude_messages::request::transform_typed(
            request,
            model,
            false,
        )?;
    let mut value = serde_json::to_value(transformed)?;
    value
        .as_object_mut()
        .expect("typed Claude request is an object")
        .remove("max_tokens");
    Ok(serde_json::from_value(value)?)
}

fn wrap_gemini(
    mut request: gemini::GenerateContentRequest,
    model: &str,
) -> Result<gemini::CountTokensRequest, TransformError> {
    request.model = Some(model.to_owned());
    Ok(crate::wire!(gemini::CountTokensRequest {
        model: Some(model.to_owned()),
        contents: Vec::new(),
        generate_content_request: Some(request),
        rest: Default::default(),
    }))
}

fn gemini_generation(
    input: gemini::CountTokensRequest,
    model: &str,
) -> gemini::GenerateContentRequest {
    input
        .generate_content_request
        .unwrap_or(crate::wire!(gemini::GenerateContentRequest {
            model: input.model.or_else(|| Some(model.to_owned())),
            contents: input.contents,
            ..Default::default()
        }))
}

pub(crate) fn gemini_response_to_openai(
    input: gemini::CountTokensResponse,
) -> openai::ResponseInputTokensResponse {
    crate::wire!(openai::ResponseInputTokensResponse {
        input_tokens: nonnegative_u32(input.total_tokens),
        object: openai::ResponseInputTokensObjectType::ResponseInputTokens,
        rest: Default::default(),
    })
}

pub(crate) fn openai_response_to_gemini(
    input: openai::ResponseInputTokensResponse,
) -> gemini::CountTokensResponse {
    count_to_gemini(u64::from(input.input_tokens))
}

pub(crate) fn gemini_response_to_claude(
    input: gemini::CountTokensResponse,
) -> claude::CountTokensResponseBody {
    crate::wire!(claude::CountTokensResponseBody {
        input_tokens: u64::from(nonnegative_u32(input.total_tokens)),
        context_management: None,
        rest: Default::default(),
    })
}

pub(crate) fn claude_response_to_gemini(
    input: claude::CountTokensResponseBody,
) -> gemini::CountTokensResponse {
    count_to_gemini(input.input_tokens)
}

fn count_to_gemini(input_tokens: u64) -> gemini::CountTokensResponse {
    crate::wire!(gemini::CountTokensResponse {
        total_tokens: Some(i32::try_from(input_tokens).unwrap_or(i32::MAX)),
        cached_content_token_count: None,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        rest: Default::default(),
    })
}

fn nonnegative_u32(value: Option<i32>) -> u32 {
    value
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
