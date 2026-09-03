use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, openai};

use crate::TransformError;

pub(crate) fn compact_request(
    target: ContentGenerationKind,
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    if target == ContentGenerationKind::ClaudeMessages {
        return crate::compact::openai_to_claude_messages::request::transform(body, model);
    }
    let responses = compact_to_responses(body, model)?;
    match target {
        ContentGenerationKind::OpenAiResponses => Ok(responses),
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_responses_to_openai_chat::request::transform(
                responses, model, stream,
            )
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::openai_responses_to_gemini_generate_content::request::transform(
                responses, model, stream,
            )
        }
        ContentGenerationKind::ClaudeMessages => unreachable!("handled before normalization"),
        ContentGenerationKind::OpenAiResponsesWebSocket => Err(TransformError::shape(
            "compact",
            "websocket is an envelope, not a compact semantic target",
        )),
    }
}

pub(crate) fn compact_response(
    target: ContentGenerationKind,
    body: Bytes,
) -> Result<Bytes, TransformError> {
    let responses = match target {
        ContentGenerationKind::OpenAiResponses => body,
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_responses_to_openai_chat::response::transform(body)?
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::openai_responses_to_gemini_generate_content::response::transform(body)?
        }
        ContentGenerationKind::ClaudeMessages => {
            return crate::compact::openai_to_claude_messages::response::transform(body);
        }
        ContentGenerationKind::OpenAiResponsesWebSocket => {
            return Err(TransformError::shape("compact", "websocket response envelope"));
        }
    };
    crate::compact::openai_to_claude_messages::response::from_responses(responses)
}

pub(crate) fn content_request(
    source: ContentGenerationKind,
    body: Bytes,
    model: &str,
) -> Result<Bytes, TransformError> {
    let responses = to_responses_request(source, body, model)?;
    let input: openai::ResponseCreateRequest = serde_json::from_slice(&responses)?;
    encode(&openai::CompactResponseRequestBody {
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
    let responses = compact_object_to_responses(body)?;
    match source {
        ContentGenerationKind::OpenAiResponses => Ok(responses),
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_chat_to_openai_responses::response::transform(responses)
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::gemini_generate_content_to_openai_responses::response::transform(responses)
        }
        ContentGenerationKind::ClaudeMessages => {
            crate::generate_content::claude_messages_to_openai_responses::response::transform(responses)
        }
        ContentGenerationKind::OpenAiResponsesWebSocket => {
            Err(TransformError::shape("compact", "websocket response envelope"))
        }
    }
}

fn compact_to_responses(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: openai::CompactResponseRequestBody = serde_json::from_slice(&body)?;
    encode(&openai::ResponseCreateRequest {
        input: input.input,
        instructions: input.instructions,
        model: input.model.or_else(|| Some(model.into())),
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier,
        stream: Some(false),
        rest: Default::default(),
        ..Default::default()
    })
}

fn to_responses_request(
    source: ContentGenerationKind,
    body: Bytes,
    model: &str,
) -> Result<Bytes, TransformError> {
    match source {
        ContentGenerationKind::OpenAiResponses => Ok(body),
        ContentGenerationKind::OpenAiChat => {
            crate::generate_content::openai_chat_to_openai_responses::request::transform(
                body, model, false,
            )
        }
        ContentGenerationKind::GeminiGenerateContent => {
            crate::generate_content::gemini_generate_content_to_openai_responses::request::transform(
                body, model, false,
            )
        }
        ContentGenerationKind::ClaudeMessages => {
            crate::generate_content::claude_messages_to_openai_responses::request::transform(
                body, model, false,
            )
        }
        ContentGenerationKind::OpenAiResponsesWebSocket => Err(TransformError::shape(
            "compact",
            "websocket request envelope",
        )),
    }
}

fn compact_object_to_responses(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai::CompactedResponseObject = serde_json::from_slice(&body)?;
    let output = input
        .output
        .into_iter()
        .filter_map(|item| compact_item_to_response(item).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    encode(&openai::ResponseObject {
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
    })
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
                            openai::ResponseOutputText {
                                type_: openai::ResponseOutputTextType::OutputText,
                                annotations: Vec::new(),
                                logprobs: None,
                                text: part.text,
                                rest: Default::default(),
                            },
                        ))
                    }
                    _ => None,
                })
                .collect();
            Some(openai::ResponseItem::Message(
                openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
                    type_: message.type_,
                    id: message.id.unwrap_or_else(|| "msg_compact".into()),
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content,
                    status: message.status,
                    phase: message.phase,
                    rest: Default::default(),
                }),
            ))
        }
        openai::CompactResponseItem::Message(_) => None,
    })
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
