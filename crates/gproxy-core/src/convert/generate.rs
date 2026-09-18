//! Buffered content generation across dialects: decode the client's request,
//! prepare the directed edge with continuation state, POST once through the
//! attempt-bound upstream, and encode the client-dialect response.

use crate::{AttemptUpstream, ProtocolState};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    adapt::generate::{
        GenerationIdentity, GenerationOutcome, GenerationProgress, GenerationStateAccess,
        chat_claude::{ChatViaClaude, ClaudeViaChat},
        chat_gemini::{ChatViaGemini, GeminiViaChat},
        chat_responses::{ChatReturnFacts, ChatViaResponses, ResponsesViaChat},
        claude_gemini::{ClaudeViaGemini, GeminiViaClaude},
        claude_responses::{ClaudeReturnFacts, ClaudeViaResponses, ResponsesViaClaude},
        gemini_responses::{GeminiReturnFacts, GeminiViaResponses, ResponsesViaGemini},
    },
    codec::{CodecLimits, decode_json, encode_json},
    transform::{
        TransformError, TransformErrorKind,
        generate::{claude_chat, claude_gemini, claude_responses, gemini_chat, gemini_responses},
        identity::IdNamespace,
    },
    wire::{claude::generate_content as c, gemini as g, openai::chat as h, openai::responses as r},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::{Serialize, de::DeserializeOwned};

pub(crate) enum Converted {
    Success(WireResponse<HttpBody>),
    Rejected(WireResponse<HttpBody>),
}

fn codec(error: gproxy_protocol::codec::CodecError) -> TransformError {
    TransformError::with_source(
        if error.kind() == gproxy_protocol::codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else {
            TransformErrorKind::InvalidInput
        },
        "client.body",
        error.to_string(),
        error,
    )
}

fn decode<T: DeserializeOwned>(body: &[u8], limits: CodecLimits) -> Result<T, TransformError> {
    decode_json(body, limits).map_err(codec)
}

fn finish<Cl: Serialize>(
    outcome: GenerationOutcome<Cl>,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    Ok(match outcome {
        GenerationOutcome::Success { response, .. } => {
            let body = encode_json(&response.body, limits).map_err(codec)?;
            Converted::Success(WireResponse {
                status: response.status,
                headers: response.headers,
                body: HttpBody::Bytes(body),
            })
        }
        GenerationOutcome::Rejected(response) => Converted::Rejected(WireResponse {
            status: response.status,
            headers: response.headers,
            body: HttpBody::Bytes(response.body),
        }),
    })
}

fn namespace() -> IdNamespace {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    IdNamespace(bytes)
}

fn auto() -> r::input::ToolChoice {
    r::input::ToolChoice::Mode(r::input::ToolChoiceMode::Auto)
}

fn responses_settings(input: &r::GenerateContentRequestBody) -> (bool, r::input::ToolChoice) {
    (
        input.parallel_tool_calls.flatten().unwrap_or(true),
        input.tool_choice.clone().unwrap_or_else(auto),
    )
}

/// One buffered generation on `upstream` for `client -> target`. The
/// upstream model comes from `state.target.model`; `now_ms` supplies the
/// creation timestamps some client dialects require and the upstream omits.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn buffered<C: BatchConnectionTrait + Send + Sync>(
    upstream: &AttemptUpstream<'_, C>,
    client: Dialect,
    target: Dialect,
    body: &[u8],
    limits: CodecLimits,
    state: &GenerationStateAccess<'_, ProtocolState<'_, C>>,
    now_ms: i64,
) -> Result<Converted, TransformError> {
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: target,
    };
    let endpoint = super::generate_endpoint(target, &state.target.model, false)?;
    let identities = GenerationIdentity::new(namespace(), namespace(), client, target)?;
    let created = now_ms.div_euclid(1000);
    macro_rules! run {
        ($pair:ty, $input:ty, [$($extra:expr),*], $facts:expr) => {{
            let input: $input = decode(body, limits)?;
            let mut prepared =
                <$pair>::prepare_with_state(input, endpoint, identities, state $(, $extra)*).await?;
            let mut progress = GenerationProgress::default();
            let outcome = prepared
                .invoke(upstream, &key, limits, state, &mut progress, $facts)
                .await?;
            finish(outcome, limits)
        }};
    }
    match (client, target) {
        (Dialect::OpenAiChat, Dialect::Claude) => run!(
            ChatViaClaude,
            h::GenerateContentRequestBody,
            [],
            |_: &c::GenerateContentResponseBody| Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::Claude, Dialect::OpenAiChat) => run!(
            ClaudeViaChat,
            c::GenerateContentRequestBody,
            [],
            |_: &h::GenerateContentResponseBody| Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::OpenAiChat, Dialect::Gemini) => run!(
            ChatViaGemini,
            h::GenerateContentRequestBody,
            [&Default::default()],
            |_: &g::GenerateContentResponseBody| Ok(gemini_chat::GeminiChatResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::Gemini, Dialect::OpenAiChat) => run!(
            GeminiViaChat,
            g::GenerateContentRequestBody,
            [],
            |_: &h::GenerateContentResponseBody| Ok(())
        ),
        (Dialect::OpenAiChat, Dialect::OpenAi) => run!(
            ChatViaResponses,
            h::GenerateContentRequestBody,
            [],
            |_: &r::GenerateContentResponseBody| Ok(())
        ),
        (Dialect::OpenAi, Dialect::OpenAiChat) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            let mut prepared =
                ResponsesViaChat::prepare_with_state(input, endpoint, identities, state).await?;
            let mut progress = GenerationProgress::default();
            let outcome = prepared
                .invoke(upstream, &key, limits, state, &mut progress, move |_| {
                    Ok(ChatReturnFacts {
                        parallel_tool_calls,
                        tool_choice,
                        prompt_cache_options: None,
                        usage: Default::default(),
                    })
                })
                .await?;
            finish(outcome, limits)
        }
        (Dialect::Claude, Dialect::Gemini) => run!(
            ClaudeViaGemini,
            c::GenerateContentRequestBody,
            [Default::default()],
            |native: &g::GenerateContentResponseBody| {
                let usage = native.usage_metadata.as_ref();
                Ok(claude_gemini::ClaudeGeminiUsageFacts {
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: usage.and_then(|u| u.cached_content_token_count),
                    thinking_tokens: usage.and_then(|u| u.thoughts_token_count),
                })
            }
        ),
        (Dialect::Gemini, Dialect::Claude) => run!(
            GeminiViaClaude,
            g::GenerateContentRequestBody,
            [None],
            |native: &c::GenerateContentResponseBody| Ok(claude_gemini::ClaudeGeminiUsageFacts {
                cache_creation_input_tokens: native.usage.cache_creation_input_tokens.flatten(),
                cache_read_input_tokens: native.usage.cache_read_input_tokens.flatten(),
                thinking_tokens: None,
            })
        ),
        (Dialect::Claude, Dialect::OpenAi) => run!(
            ClaudeViaResponses,
            c::GenerateContentRequestBody,
            [],
            |_: &r::GenerateContentResponseBody| Ok(
                claude_responses::ClaudeRequestContext::default()
            )
        ),
        (Dialect::OpenAi, Dialect::Claude) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            let mut prepared = ResponsesViaClaude::prepare_with_state(
                input,
                endpoint,
                identities,
                state,
                Default::default(),
            )
            .await?;
            let mut progress = GenerationProgress::default();
            let outcome = prepared
                .invoke(
                    upstream,
                    &key,
                    limits,
                    state,
                    &mut progress,
                    move |native| {
                        Ok(ClaudeReturnFacts {
                            parallel_tool_calls,
                            tool_choice,
                            prompt_cache_options: None,
                            usage: claude_responses::ResponsesUsageFacts {
                                cache_write_tokens: native
                                    .usage
                                    .cache_creation_input_tokens
                                    .flatten(),
                                cached_tokens: native.usage.cache_read_input_tokens.flatten(),
                                reasoning_tokens: None,
                            },
                            created_at: created,
                        })
                    },
                )
                .await?;
            finish(outcome, limits)
        }
        (Dialect::Gemini, Dialect::OpenAi) => run!(
            GeminiViaResponses,
            g::GenerateContentRequestBody,
            [],
            |_: &r::GenerateContentResponseBody| Ok(
                gemini_responses::GeminiReplayContext::default()
            )
        ),
        (Dialect::OpenAi, Dialect::Gemini) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            let mut prepared = ResponsesViaGemini::prepare_with_state(
                input,
                endpoint,
                identities,
                state,
                Default::default(),
            )
            .await?;
            let mut progress = GenerationProgress::default();
            let outcome = prepared
                .invoke(
                    upstream,
                    &key,
                    limits,
                    state,
                    &mut progress,
                    move |native| {
                        Ok(GeminiReturnFacts {
                            parallel_tool_calls,
                            tool_choice,
                            prompt_cache_options: None,
                            usage: gemini_responses::GeminiUsageFacts {
                                cache_write_tokens: None,
                                cached_tokens: native
                                    .usage_metadata
                                    .as_ref()
                                    .and_then(|u| u.cached_content_token_count),
                            },
                            created_at: created,
                        })
                    },
                )
                .await?;
            finish(outcome, limits)
        }
        (client, target) => Err(TransformError::unsupported(
            "generate",
            format!("no conversion from {client:?} to {target:?}"),
        )),
    }
}
