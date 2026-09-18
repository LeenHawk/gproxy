//! Buffered content generation across dialects: decode the client's request,
//! prepare the directed edge with continuation state, POST once through the
//! attempt-bound upstream, and encode the client-dialect response.

use super::{Call, Converted};
use crate::{ProtocolState, StateScope};
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
        stream::{
            ChatViaGeminiStreamFacts, ClaudeViaGeminiStreamFacts, GeminiViaClaudeStreamFacts,
            ResponsesViaClaudeStreamFacts, ResponsesViaGeminiStreamFacts, StreamSettings,
            StreamStart, StreamTarget, event::EventLimits, reader::SourceFraming,
        },
    },
    codec::{CodecLimits, decode_json, encode_json},
    connection::{ByteStream, TransportError},
    transform::{
        TransformError, TransformErrorKind,
        generate::{
            chat_responses, claude_chat, claude_gemini, claude_responses, gemini_chat,
            gemini_responses,
        },
        identity::{IdNamespace, IdentityTarget},
    },
    wire::{claude::generate_content as c, gemini as g, openai::chat as h, openai::responses as r},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::{Serialize, de::DeserializeOwned};
use std::time::SystemTime;

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

/// Finite per-stream event budgets; the codec limits already bound bytes.
const MAX_STREAM_EVENTS: usize = 65_536;
const MAX_STREAM_ITEMS: usize = 256;
const MAX_STREAM_TOOLS: usize = 256;
const MAX_STREAM_PARTS: usize = 256;
const MAX_STREAM_CHOICES: usize = 8;

fn stream_settings(
    limits: CodecLimits,
    client: Dialect,
    client_query: Option<&str>,
) -> StreamSettings {
    let client_framing = match client {
        Dialect::Gemini if !client_query.is_some_and(|q| q.split('&').any(|p| p == "alt=sse")) => {
            SourceFraming::JsonArray
        }
        _ => SourceFraming::Sse,
    };
    StreamSettings {
        codec: limits,
        events: EventLimits {
            max_events: MAX_STREAM_EVENTS,
            max_bytes: usize::try_from(limits.max_body_bytes).unwrap_or(usize::MAX),
            max_pending_bytes: usize::try_from(limits.max_value_bytes).unwrap_or(usize::MAX),
            max_items: MAX_STREAM_ITEMS,
            max_tools: MAX_STREAM_TOOLS,
            max_parts: MAX_STREAM_PARTS,
            max_choices: MAX_STREAM_CHOICES,
        },
        // Upstream streams are always requested as SSE (Gemini via `alt=sse`).
        source_framing: SourceFraming::Sse,
        client_framing,
    }
}

/// Continuation-state pieces a driven client stream carries by value so it
/// can rebuild `GenerationStateAccess` on every poll.
struct OwnedState<C> {
    store: ProtocolState<C>,
    scope: StateScope,
    target: IdentityTarget,
    conversation_key: String,
    expires_at: SystemTime,
    max_records: usize,
}

impl<C: BatchConnectionTrait + Send + Sync> OwnedState<C> {
    fn capture(call: &Call<'_, C>, state: &GenerationStateAccess<'_, ProtocolState<C>>) -> Self {
        Self {
            store: call.state_store.clone(),
            scope: call.state_scope.clone(),
            target: state.target.clone(),
            conversation_key: state.conversation_key.clone(),
            expires_at: state.expires_at,
            max_records: state.max_records,
        }
    }

    fn access(&self) -> GenerationStateAccess<'_, ProtocolState<C>> {
        GenerationStateAccess {
            store: &self.store,
            scope: &self.scope,
            target: self.target.clone(),
            conversation_key: self.conversation_key.clone(),
            expires_at: self.expires_at,
            now: SystemTime::now(),
            max_records: self.max_records,
        }
    }
}

fn rejected(response: WireResponse<gproxy_protocol::connection::Bytes>) -> Converted {
    Converted::Rejected(WireResponse {
        status: response.status,
        headers: response.headers,
        body: HttpBody::Bytes(response.body),
    })
}

fn transport(error: TransformError) -> TransportError {
    Box::new(std::io::Error::other(error.to_string()))
}

/// Drive one started invocation as a client byte stream. A macro rather than
/// a generic function: `StreamInvocation::next` is only `Send` per concrete
/// bridge, which a generic bound cannot express.
macro_rules! drive {
    ($invocation:expr, $owned:expr) => {{
        let invocation = $invocation;
        let owned = $owned;
        let stream: ByteStream = Box::pin(futures_util::stream::unfold(
            Some((invocation, owned)),
            |slot| async move {
                let (mut invocation, owned) = slot?;
                let next = invocation.next(&owned.access()).await;
                match next {
                    Ok(Some(chunk)) => Some((Ok(chunk.bytes), Some((invocation, owned)))),
                    Ok(None) => None,
                    Err(error) => Some((Err(transport(error)), None)),
                }
            },
        ));
        stream
    }};
}

/// One incremental generation for `client -> target`: prepare the streaming
/// edge with continuation state, POST once, and hand back a client-dialect
/// stream that is driven chunk by chunk; the request settles when it ends.
pub(crate) async fn streamed<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let client = call.client.dialect;
    let target = call.target;
    let upstream = call.upstream;
    let body = call.body();
    let limits = call.limits;
    let state = &call.generation_state()?;
    let key = OperationKey {
        operation: Operation::StreamGenerateContent,
        dialect: target,
    };
    let endpoint = super::generate_endpoint(target, &state.target.model, true)?;
    let identities = GenerationIdentity::new(namespace(), namespace(), client, target)?;
    let stream_target = StreamTarget {
        endpoint,
        identities,
    };
    let settings = stream_settings(limits, client, call.request.query.as_deref());
    let created = call.now_ms.div_euclid(1000);
    let model = state.target.model.clone();
    macro_rules! run {
        ($pair:ty, $input:ty, |$input_name:ident| $ctx:expr) => {{
            let $input_name: $input = decode(body, limits)?;
            let context = $ctx;
            // Boxed per pair: twelve inlined stream state machines would
            // otherwise sit side by side in one oversized frame.
            Box::pin(async move {
                let invocation =
                    <$pair>::prepare_stream($input_name, stream_target, context, settings, state)
                        .await?;
                run!(@start invocation)
            })
            .await
        }};
        ($pair:ty, $input:ty) => {{
            let input: $input = decode(body, limits)?;
            Box::pin(async move {
                let invocation =
                    <$pair>::prepare_stream(input, stream_target, settings, state).await?;
                run!(@start invocation)
            })
            .await
        }};
        (@start $invocation:expr) => {{
            let mut invocation = $invocation;
            match invocation.start(upstream, &key, state).await? {
                StreamStart::Rejected(response) => Ok(rejected(response)),
                StreamStart::Streaming(head) => {
                    let owned = OwnedState::capture(call, state);
                    Ok(Converted::Stream(WireResponse {
                        status: head.status,
                        headers: head.headers,
                        body: drive!(invocation, owned),
                    }))
                }
            }
        }};
    }
    match (client, target) {
        (Dialect::OpenAiChat, Dialect::Claude) => {
            run!(ChatViaClaude, h::GenerateContentRequestBody, |input| {
                claude_chat::stream::ClaudeToChatContext { created }
            })
        }
        (Dialect::Claude, Dialect::OpenAiChat) => {
            run!(ClaudeViaChat, c::GenerateContentRequestBody, |input| {
                claude_chat::stream::ChatToClaudeContext::default()
            })
        }
        (Dialect::OpenAiChat, Dialect::OpenAi) => {
            run!(ChatViaResponses, h::GenerateContentRequestBody)
        }
        (Dialect::OpenAi, Dialect::OpenAiChat) => {
            run!(ResponsesViaChat, r::GenerateContentRequestBody, |input| {
                let (parallel_tool_calls, tool_choice) = responses_settings(&input);
                let context: chat_responses::stream::ChatToResponsesContext =
                    chat_responses::ResponsesResponseContext {
                        request: input.clone(),
                        effective_parallel_tool_calls: parallel_tool_calls,
                        effective_tool_choice: tool_choice,
                        usage: Default::default(),
                        effective_prompt_cache_options: None,
                    }
                    .into();
                context
            })
        }
        (Dialect::Gemini, Dialect::OpenAiChat) => {
            run!(GeminiViaChat, g::GenerateContentRequestBody)
        }
        (Dialect::OpenAiChat, Dialect::Gemini) => {
            run!(ChatViaGemini, h::GenerateContentRequestBody, |input| {
                ChatViaGeminiStreamFacts {
                    function_names: Default::default(),
                    response: gemini_chat::stream::GeminiToChatContext {
                        created,
                        model: Some(model.clone()),
                    },
                }
            })
        }
        (Dialect::Claude, Dialect::Gemini) => {
            run!(ClaudeViaGemini, c::GenerateContentRequestBody, |input| {
                ClaudeViaGeminiStreamFacts {
                    request: Default::default(),
                    response: claude_gemini::stream::GeminiToClaudeContext {
                        model: Some(model.clone()),
                        ..Default::default()
                    },
                }
            })
        }
        (Dialect::Gemini, Dialect::Claude) => {
            run!(GeminiViaClaude, g::GenerateContentRequestBody, |input| {
                GeminiViaClaudeStreamFacts {
                    max_tokens: None,
                    response: claude_gemini::stream::ClaudeToGeminiContext::default(),
                }
            })
        }
        (Dialect::Claude, Dialect::OpenAi) => {
            run!(ClaudeViaResponses, c::GenerateContentRequestBody, |input| {
                claude_responses::stream::ResponsesToClaudeContext::default()
            })
        }
        (Dialect::OpenAi, Dialect::Claude) => {
            run!(ResponsesViaClaude, r::GenerateContentRequestBody, |input| {
                let (parallel_tool_calls, tool_choice) = responses_settings(&input);
                ResponsesViaClaudeStreamFacts {
                    request: Default::default(),
                    response: claude_responses::ClaudeResponseContext {
                        request: input.clone(),
                        effective_parallel_tool_calls: parallel_tool_calls,
                        effective_tool_choice: tool_choice,
                        usage: Default::default(),
                        created_at: created,
                        effective_prompt_cache_options: None,
                    }
                    .into(),
                }
            })
        }
        (Dialect::Gemini, Dialect::OpenAi) => {
            run!(GeminiViaResponses, g::GenerateContentRequestBody, |input| {
                gemini_responses::stream::ResponsesToGeminiContext::default()
            })
        }
        (Dialect::OpenAi, Dialect::Gemini) => {
            run!(ResponsesViaGemini, r::GenerateContentRequestBody, |input| {
                let (parallel_tool_calls, tool_choice) = responses_settings(&input);
                ResponsesViaGeminiStreamFacts {
                    request: Default::default(),
                    response: gemini_responses::stream::GeminiToResponsesContext {
                        response: gemini_responses::GeminiResponseContext {
                            request: input.clone(),
                            effective_parallel_tool_calls: parallel_tool_calls,
                            effective_tool_choice: tool_choice,
                            usage: Default::default(),
                            created_at: created,
                            effective_prompt_cache_options: None,
                        },
                        actual_model: Some(model.clone()),
                        final_thinking_tokens: None,
                    },
                }
            })
        }
        (client, target) => Err(TransformError::unsupported(
            "generate.stream",
            format!("no streaming conversion from {client:?} to {target:?}"),
        )),
    }
}

/// One buffered generation for `client -> target`. The upstream model comes
/// from the continuation state target; `now_ms` supplies the creation
/// timestamps some client dialects require and the upstream omits.
pub(crate) async fn buffered<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let client = call.client.dialect;
    let target = call.target;
    let upstream = call.upstream;
    let body = call.body();
    let limits = call.limits;
    let state = &call.generation_state()?;
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: target,
    };
    let endpoint = super::generate_endpoint(target, &state.target.model, false)?;
    let identities = GenerationIdentity::new(namespace(), namespace(), client, target)?;
    let created = call.now_ms.div_euclid(1000);
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
