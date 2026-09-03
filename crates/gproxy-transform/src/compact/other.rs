use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, openai};

use crate::TransformError;

pub(crate) fn compact_request(
    target: ContentGenerationKind,
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    let input: openai::CompactResponseRequestBody = serde_json::from_slice(&body)?;
    match target {
        ContentGenerationKind::OpenAiResponses => {
            encode(&compact_to_responses_typed(input, model))
        }
        ContentGenerationKind::OpenAiChat => encode(
            &crate::generate_content::openai_responses_to_openai_chat::request::transform_typed(
                compact_to_responses_typed(input, model),
                model,
                stream,
            )?,
        ),
        ContentGenerationKind::GeminiGenerateContent => encode(
            &crate::generate_content::openai_responses_to_gemini_generate_content::request::transform_typed(
                compact_to_responses_typed(input, model),
                model,
                stream,
            )?,
        ),
        ContentGenerationKind::ClaudeMessages => encode(
            &crate::compact::openai_to_claude_messages::request::transform_typed(input, model)?,
        ),
        ContentGenerationKind::OpenAiResponsesWebSocket => Err(TransformError::shape(
            "compact",
            "websocket is an envelope, not a compact semantic target",
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => return Err(crate::TransformError::unsupported("protocol enum", "unrecognized external variant")),
    }
}

pub(crate) fn compact_response(
    target: ContentGenerationKind,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    let responses = match target {
        ContentGenerationKind::OpenAiResponses => serde_json::from_slice(&body)?,
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_responses_to_openai_chat::response::transform_typed(
                serde_json::from_slice(&body)?,
            )?
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::openai_responses_to_gemini_generate_content::response::transform_typed(
                serde_json::from_slice(&body)?,
            )?
        }
        ContentGenerationKind::ClaudeMessages => {
            return encode(
                &crate::compact::openai_to_claude_messages::response::transform_typed(
                    serde_json::from_slice(&body)?,
                )?,
            );
        }
        ContentGenerationKind::OpenAiResponsesWebSocket => {
            return Err(TransformError::shape("compact", "websocket response envelope"));
        },
        #[cfg(not(feature = "exhaustive"))]
        _ => return Err(crate::TransformError::unsupported("protocol enum", "unrecognized external variant"))
    };
    encode(&crate::compact::openai_to_claude_messages::response::from_responses_typed(responses)?)
}

pub(crate) fn content_request(
    source: ContentGenerationKind,
    body: Bytes,
    model: &str,
) -> Result<Bytes, TransformError> {
    let input = to_responses_request(source, body, model)?;
    encode(&responses_to_compact_request_typed(input))
}

pub(crate) fn responses_to_compact_request_typed(
    input: openai::ResponseCreateRequest,
) -> openai::CompactResponseRequestBody {
    crate::wire!(openai::CompactResponseRequestBody {
        input: input.input,
        instructions: input.instructions,
        model: input.model,
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier,
        rest: Default::default(),
    })
}

pub(crate) fn content_response(
    source: ContentGenerationKind,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    let compact: openai::CompactedResponseObject = serde_json::from_slice(&body)?;
    let responses = compact_object_to_responses_typed(compact)?;
    match source {
        ContentGenerationKind::OpenAiResponses => encode(&responses),
        ContentGenerationKind::OpenAiChat => encode(
            &crate::generate_content::openai_chat_to_openai_responses::response::transform_typed(
                responses,
            )?,
        ),
        ContentGenerationKind::GeminiGenerateContent => encode(
            &crate::generate_content::gemini_generate_content_to_openai_responses::response::transform_typed(
                responses,
            )?,
        ),
        ContentGenerationKind::ClaudeMessages => encode(
            &crate::generate_content::claude_messages_to_openai_responses::response::transform_typed(
                responses,
            )?,
        ),
        ContentGenerationKind::OpenAiResponsesWebSocket => {
            Err(TransformError::shape("compact", "websocket response envelope"))
        },
        #[cfg(not(feature = "exhaustive"))]
        _ => return Err(crate::TransformError::unsupported("protocol enum", "unrecognized external variant"))
    }
}

pub(crate) fn compact_to_responses_typed(
    input: openai::CompactResponseRequestBody,
    model: &str,
) -> openai::ResponseCreateRequest {
    crate::wire!(openai::ResponseCreateRequest {
        input: input.input,
        instructions: input.instructions,
        model: input.model.or_else(|| Some(model.into())),
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier,
        stream: Some(false),
        max_output_tokens: Some(32_768),
        rest: Default::default(),
        ..Default::default()
    })
}

fn to_responses_request(
    source: ContentGenerationKind,
    body: Bytes,
    model: &str,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    match source {
        ContentGenerationKind::OpenAiResponses => Ok(serde_json::from_slice(&body)?),
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_chat_to_openai_responses::request::transform_typed(
                serde_json::from_slice(&body)?,
                model,
                false,
            )
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::gemini_generate_content_to_openai_responses::request::transform_typed(
                serde_json::from_slice(&body)?,
                model,
                false,
            )
        }
        ContentGenerationKind::ClaudeMessages => {
            crate::generate_content::claude_messages_to_openai_responses::request::transform_typed(
                serde_json::from_slice(&body)?,
                model,
                false,
            )
        }
        ContentGenerationKind::OpenAiResponsesWebSocket => Err(TransformError::shape(
            "compact",
            "websocket request envelope",
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => return Err(crate::TransformError::unsupported("protocol enum", "unrecognized external variant")),
    }
}

pub(crate) fn compact_object_to_responses_typed(
    input: openai::CompactedResponseObject,
) -> Result<openai::ResponseObject, TransformError> {
    let output = input
        .output
        .into_iter()
        .filter_map(|item| compact_item_to_response(item).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crate::wire!(openai::ResponseObject {
        id: input.id,
        created_at: input.created_at,
        background: None,
        completed_at: None,
        conversation: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: None,
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output,
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        status: Some(openai::ResponseStatus::Completed),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: Some(input.usage),
        user: None,
        rest: Default::default(),
    }))
}

fn compact_item_to_response(
    item: openai::CompactResponseItem,
) -> Result<Option<openai::ResponseItem>, TransformError> {
    Ok(match item {
        openai::CompactResponseItem::Typed(item) => Some(openai::ResponseItem::Typed(item)),
        openai::CompactResponseItem::Unknown(_) => None,
        openai::CompactResponseItem::Message(message)
            if message.role == openai::CompactMessageRole::Assistant =>
        {
            let content = message
                .content
                .into_iter()
                .filter_map(|part| match part {
                    openai::CompactMessageContentPart::Text(part) => {
                        Some(openai::ResponseMessageOutputContentPart::OutputText(
                            crate::wire!(openai::ResponseOutputText {
                                type_: openai::ResponseOutputTextType::OutputText,
                                annotations: Vec::new(),
                                logprobs: None,
                                text: part.text,
                                rest: Default::default(),
                            }),
                        ))
                    }
                    _ => None,
                })
                .collect();
            Some(openai::ResponseItem::Message(
                openai::ResponseMessageItem::Output(crate::wire!(
                    openai::ResponseOutputMessageItem {
                        type_: message.type_,
                        id: message.id.unwrap_or_else(|| "msg_compact".into()),
                        role: openai::ResponseOutputMessageRole::Assistant,
                        content,
                        status: message.status,
                        phase: message.phase,
                        rest: Default::default(),
                    }
                )),
            ))
        }
        openai::CompactResponseItem::Message(_) => None,
    })
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
