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
        (WireFamily::OpenAi, WireFamily::Gemini) => openai_to_gemini(body, model),
        (WireFamily::Claude, WireFamily::Gemini) => claude_to_gemini(body, model),
        (WireFamily::Gemini, WireFamily::OpenAi) => gemini_to_openai(body, model),
        (WireFamily::Gemini, WireFamily::Claude) => gemini_to_claude(body, model),
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
            encode(&openai::ResponseInputTokensResponse {
                input_tokens: nonnegative_u32(input.total_tokens),
                object: openai::ResponseInputTokensObjectType::ResponseInputTokens,
                rest: input.rest,
            })
        }
        (WireFamily::Gemini, WireFamily::OpenAi) => {
            let input: openai::ResponseInputTokensResponse = serde_json::from_slice(&body)?;
            encode(&gemini::CountTokensResponse {
                total_tokens: Some(i32::try_from(input.input_tokens).unwrap_or(i32::MAX)),
                cached_content_token_count: None,
                prompt_tokens_details: Vec::new(),
                cache_tokens_details: Vec::new(),
                rest: input.rest,
            })
        }
        (WireFamily::Claude, WireFamily::Gemini) => {
            let input: gemini::CountTokensResponse = serde_json::from_slice(&body)?;
            encode(&claude::CountTokensResponseBody {
                input_tokens: u64::from(nonnegative_u32(input.total_tokens)),
                context_management: None,
                rest: input.rest,
            })
        }
        (WireFamily::Gemini, WireFamily::Claude) => {
            let input: claude::CountTokensResponseBody = serde_json::from_slice(&body)?;
            encode(&gemini::CountTokensResponse {
                total_tokens: Some(i32::try_from(input.input_tokens).unwrap_or(i32::MAX)),
                cached_content_token_count: None,
                prompt_tokens_details: Vec::new(),
                cache_tokens_details: Vec::new(),
                rest: input.rest,
            })
        }
        _ => Err(TransformError::shape(
            "count tokens",
            "unsupported Gemini pair",
        )),
    }
}

fn openai_to_gemini(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: openai::ResponseInputTokensRequest = serde_json::from_slice(&body)?;
    let response: openai::ResponseCreateRequest =
        serde_json::from_value(serde_json::to_value(input)?)?;
    let transformed =
        crate::generate_content::openai_responses_to_gemini_generate_content::request::transform(
            Bytes::from(serde_json::to_vec(&response)?),
            model,
            false,
        )?;
    wrap_gemini(transformed, model)
}

fn claude_to_gemini(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: claude::CountTokensRequestBody = serde_json::from_slice(&body)?;
    let mut value = serde_json::to_value(input)?;
    value
        .as_object_mut()
        .expect("typed Claude request is an object")
        .insert("max_tokens".into(), serde_json::json!(1));
    let transformed =
        crate::generate_content::claude_messages_to_gemini_generate_content::request::transform(
            Bytes::from(serde_json::to_vec(&value)?),
            model,
            false,
        )?;
    wrap_gemini(transformed, model)
}

fn gemini_to_openai(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let request = gemini_generation(body, model)?;
    let transformed =
        crate::generate_content::gemini_generate_content_to_openai_responses::request::transform(
            Bytes::from(serde_json::to_vec(&request)?),
            model,
            false,
        )?;
    let response: openai::ResponseCreateRequest = serde_json::from_slice(&transformed)?;
    encode(
        &serde_json::from_value::<openai::ResponseInputTokensRequest>(serde_json::to_value(
            response,
        )?)?,
    )
}

fn gemini_to_claude(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let request = gemini_generation(body, model)?;
    let transformed =
        crate::generate_content::gemini_generate_content_to_claude_messages::request::transform(
            Bytes::from(serde_json::to_vec(&request)?),
            model,
            false,
        )?;
    let mut value: serde_json::Value = serde_json::from_slice(&transformed)?;
    value
        .as_object_mut()
        .expect("typed Claude request is an object")
        .remove("max_tokens");
    encode(&serde_json::from_value::<claude::CountTokensRequestBody>(
        value,
    )?)
}

fn wrap_gemini(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let mut request: gemini::GenerateContentRequest = serde_json::from_slice(&body)?;
    request.model = Some(model.to_owned());
    encode(&gemini::CountTokensRequest {
        model: Some(model.to_owned()),
        contents: Vec::new(),
        generate_content_request: Some(request),
        rest: Default::default(),
    })
}

fn gemini_generation(
    body: Bytes,
    model: &str,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    let input: gemini::CountTokensRequest = serde_json::from_slice(&body)?;
    Ok(input
        .generate_content_request
        .unwrap_or(gemini::GenerateContentRequest {
            model: input.model.or_else(|| Some(model.to_owned())),
            contents: input.contents,
            ..Default::default()
        }))
}

fn nonnegative_u32(value: Option<i32>) -> u32 {
    value
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
