//! Content-generation dispatch arms: the 12 pairs wired in M2, including
//! per-event stream conversion.

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{StreamEventOut, not_wired, run, run_ok};
use crate::protocol::{ContentGenerationKind, OperationKey, claude, openai};
use crate::transform::generate_content as gc;
use crate::transform::{TransformContext, TransformError, TransformPair};

/// Whether this pair belongs to the content-generation group.
pub(super) fn is_content(pair: TransformPair) -> bool {
    use TransformPair as P;
    matches!(
        pair,
        P::ClaudeMessagesToGeminiGenerateContent
            | P::ClaudeMessagesToOpenAiChat
            | P::ClaudeMessagesToOpenAiResponses
            | P::GeminiGenerateContentToClaudeMessages
            | P::GeminiGenerateContentToOpenAiChat
            | P::GeminiGenerateContentToOpenAiResponses
            | P::OpenAiChatToClaudeMessages
            | P::OpenAiChatToGeminiGenerateContent
            | P::OpenAiChatToOpenAiResponses
            | P::OpenAiResponsesToOpenAiResponsesWebSocket
            | P::OpenAiResponsesWebSocketToOpenAiResponses
            | P::OpenAiChatToOpenAiResponsesWebSocket
            | P::OpenAiResponsesWebSocketToOpenAiChat
            | P::ClaudeMessagesToOpenAiResponsesWebSocket
            | P::OpenAiResponsesWebSocketToClaudeMessages
            | P::GeminiGenerateContentToOpenAiResponsesWebSocket
            | P::OpenAiResponsesWebSocketToGeminiGenerateContent
            | P::OpenAiResponsesToClaudeMessages
            | P::OpenAiResponsesToGeminiGenerateContent
            | P::OpenAiResponsesToOpenAiChat
    )
}

pub(super) fn request_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    use TransformPair as P;
    match pair {
        P::ClaudeMessagesToGeminiGenerateContent => run(
            gc::claude_messages_to_gemini_generate_content::request,
            ctx,
            body,
        ),
        P::ClaudeMessagesToOpenAiChat => {
            run(gc::claude_messages_to_openai_chat::request, ctx, body)
        }
        P::ClaudeMessagesToOpenAiResponses => {
            run(gc::claude_messages_to_openai_responses::request, ctx, body)
        }
        P::GeminiGenerateContentToClaudeMessages => run(
            gc::gemini_generate_content_to_claude_messages::request,
            ctx,
            body,
        ),
        P::GeminiGenerateContentToOpenAiChat => run(
            gc::gemini_generate_content_to_openai_chat::request,
            ctx,
            body,
        ),
        P::GeminiGenerateContentToOpenAiResponses => run(
            gc::gemini_generate_content_to_openai_responses::request,
            ctx,
            body,
        ),
        P::OpenAiChatToClaudeMessages => {
            run(gc::openai_chat_to_claude_messages::request, ctx, body)
        }
        P::OpenAiChatToGeminiGenerateContent => run(
            gc::openai_chat_to_gemini_generate_content::request,
            ctx,
            body,
        ),
        P::OpenAiChatToOpenAiResponses => {
            run(gc::openai_chat_to_openai_responses::request, ctx, body)
        }
        P::OpenAiResponsesToOpenAiResponsesWebSocket => run(
            gc::openai_responses_websocket::http_request_to_ws_request,
            ctx,
            body,
        ),
        P::OpenAiResponsesWebSocketToOpenAiResponses => run(
            gc::openai_responses_websocket::ws_request_to_http_request,
            ctx,
            body,
        ),
        P::OpenAiChatToOpenAiResponsesWebSocket => {
            let body = request_via_openai_responses(
                gc::openai_chat_to_openai_responses::request,
                ctx,
                body,
            )?;
            run(
                gc::openai_responses_websocket::http_request_to_ws_request,
                &responses_to_target_ctx(ctx),
                &body,
            )
        }
        P::OpenAiResponsesWebSocketToOpenAiChat => {
            let body = run(
                gc::openai_responses_websocket::ws_request_to_http_request,
                &source_to_responses_ctx(ctx),
                body,
            )?;
            request_from_openai_responses(gc::openai_responses_to_openai_chat::request, ctx, &body)
        }
        P::ClaudeMessagesToOpenAiResponsesWebSocket => {
            let body = request_via_openai_responses(
                gc::claude_messages_to_openai_responses::request,
                ctx,
                body,
            )?;
            run(
                gc::openai_responses_websocket::http_request_to_ws_request,
                &responses_to_target_ctx(ctx),
                &body,
            )
        }
        P::OpenAiResponsesWebSocketToClaudeMessages => {
            let body = run(
                gc::openai_responses_websocket::ws_request_to_http_request,
                &source_to_responses_ctx(ctx),
                body,
            )?;
            request_from_openai_responses(
                gc::openai_responses_to_claude_messages::request,
                ctx,
                &body,
            )
        }
        P::GeminiGenerateContentToOpenAiResponsesWebSocket => {
            let body = request_via_openai_responses(
                gc::gemini_generate_content_to_openai_responses::request,
                ctx,
                body,
            )?;
            run(
                gc::openai_responses_websocket::http_request_to_ws_request,
                &responses_to_target_ctx(ctx),
                &body,
            )
        }
        P::OpenAiResponsesWebSocketToGeminiGenerateContent => {
            let body = run(
                gc::openai_responses_websocket::ws_request_to_http_request,
                &source_to_responses_ctx(ctx),
                body,
            )?;
            request_from_openai_responses(
                gc::openai_responses_to_gemini_generate_content::request,
                ctx,
                &body,
            )
        }
        P::OpenAiResponsesToClaudeMessages => {
            run(gc::openai_responses_to_claude_messages::request, ctx, body)
        }
        P::OpenAiResponsesToGeminiGenerateContent => run(
            gc::openai_responses_to_gemini_generate_content::request,
            ctx,
            body,
        ),
        P::OpenAiResponsesToOpenAiChat => {
            run(gc::openai_responses_to_openai_chat::request, ctx, body)
        }
        other => Err(not_wired(other)),
    }
}

pub(super) fn response_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    use TransformPair as P;
    match pair {
        P::ClaudeMessagesToGeminiGenerateContent => run(
            gc::claude_messages_to_gemini_generate_content::response,
            ctx,
            body,
        ),
        P::ClaudeMessagesToOpenAiChat => {
            run(gc::claude_messages_to_openai_chat::response, ctx, body)
        }
        P::ClaudeMessagesToOpenAiResponses => {
            run(gc::claude_messages_to_openai_responses::response, ctx, body)
        }
        P::GeminiGenerateContentToClaudeMessages => run(
            gc::gemini_generate_content_to_claude_messages::response,
            ctx,
            body,
        ),
        P::GeminiGenerateContentToOpenAiChat => run(
            gc::gemini_generate_content_to_openai_chat::response,
            ctx,
            body,
        ),
        P::GeminiGenerateContentToOpenAiResponses => run(
            gc::gemini_generate_content_to_openai_responses::response,
            ctx,
            body,
        ),
        P::OpenAiChatToClaudeMessages => {
            run(gc::openai_chat_to_claude_messages::response, ctx, body)
        }
        P::OpenAiChatToGeminiGenerateContent => run(
            gc::openai_chat_to_gemini_generate_content::response,
            ctx,
            body,
        ),
        P::OpenAiChatToOpenAiResponses => {
            run(gc::openai_chat_to_openai_responses::response, ctx, body)
        }
        P::OpenAiResponsesToOpenAiResponsesWebSocket
        | P::OpenAiResponsesWebSocketToOpenAiResponses => {
            run_ok(gc::openai_responses_websocket::identity, ctx, body)
        }
        P::OpenAiChatToOpenAiResponsesWebSocket => {
            response_via_openai_responses(gc::openai_chat_to_openai_responses::response, ctx, body)
        }
        P::OpenAiResponsesWebSocketToOpenAiChat => {
            response_from_openai_responses(gc::openai_responses_to_openai_chat::response, ctx, body)
        }
        P::ClaudeMessagesToOpenAiResponsesWebSocket => response_via_openai_responses(
            gc::claude_messages_to_openai_responses::response,
            ctx,
            body,
        ),
        P::OpenAiResponsesWebSocketToClaudeMessages => response_from_openai_responses(
            gc::openai_responses_to_claude_messages::response,
            ctx,
            body,
        ),
        P::GeminiGenerateContentToOpenAiResponsesWebSocket => response_via_openai_responses(
            gc::gemini_generate_content_to_openai_responses::response,
            ctx,
            body,
        ),
        P::OpenAiResponsesWebSocketToGeminiGenerateContent => response_from_openai_responses(
            gc::openai_responses_to_gemini_generate_content::response,
            ctx,
            body,
        ),
        P::OpenAiResponsesToClaudeMessages => {
            run(gc::openai_responses_to_claude_messages::response, ctx, body)
        }
        P::OpenAiResponsesToGeminiGenerateContent => run(
            gc::openai_responses_to_gemini_generate_content::response,
            ctx,
            body,
        ),
        P::OpenAiResponsesToOpenAiChat => {
            run(gc::openai_responses_to_openai_chat::response, ctx, body)
        }
        other => Err(not_wired(other)),
    }
}

pub(super) fn stream_event(
    pair: TransformPair,
    ctx: &TransformContext,
    data: &str,
) -> Result<StreamEventOut, TransformError> {
    use TransformPair as P;
    match pair {
        P::ClaudeMessagesToGeminiGenerateContent => to_plain(
            gc::claude_messages_to_gemini_generate_content::stream_event,
            ctx,
            data,
        ),
        P::ClaudeMessagesToOpenAiChat => {
            to_plain(gc::claude_messages_to_openai_chat::stream_event, ctx, data)
        }
        P::ClaudeMessagesToOpenAiResponses => to_responses(
            gc::claude_messages_to_openai_responses::stream_event,
            ctx,
            data,
        ),
        P::GeminiGenerateContentToClaudeMessages => to_claude(
            gc::gemini_generate_content_to_claude_messages::stream_event,
            ctx,
            data,
        ),
        P::GeminiGenerateContentToOpenAiChat => to_plain(
            gc::gemini_generate_content_to_openai_chat::stream_event,
            ctx,
            data,
        ),
        P::GeminiGenerateContentToOpenAiResponses => to_responses(
            gc::gemini_generate_content_to_openai_responses::stream_event,
            ctx,
            data,
        ),
        P::OpenAiChatToClaudeMessages => {
            to_claude(gc::openai_chat_to_claude_messages::stream_event, ctx, data)
        }
        P::OpenAiChatToGeminiGenerateContent => to_plain(
            gc::openai_chat_to_gemini_generate_content::stream_event,
            ctx,
            data,
        ),
        P::OpenAiChatToOpenAiResponses => {
            to_responses(gc::openai_chat_to_openai_responses::stream_event, ctx, data)
        }
        P::OpenAiResponsesToOpenAiResponsesWebSocket
        | P::OpenAiResponsesWebSocketToOpenAiResponses => {
            to_responses(|event, _| Ok(event), ctx, data)
        }
        P::OpenAiChatToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::openai_chat_to_openai_responses::stream_event,
            ctx,
            data,
        ),
        P::OpenAiResponsesWebSocketToOpenAiChat => to_plain(
            gc::openai_responses_to_openai_chat::stream_event,
            &responses_to_target_ctx(ctx),
            data,
        ),
        P::ClaudeMessagesToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::claude_messages_to_openai_responses::stream_event,
            ctx,
            data,
        ),
        P::OpenAiResponsesWebSocketToClaudeMessages => to_claude(
            gc::openai_responses_to_claude_messages::stream_event,
            &responses_to_target_ctx(ctx),
            data,
        ),
        P::GeminiGenerateContentToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::gemini_generate_content_to_openai_responses::stream_event,
            ctx,
            data,
        ),
        P::OpenAiResponsesWebSocketToGeminiGenerateContent => to_plain(
            gc::openai_responses_to_gemini_generate_content::stream_event,
            &responses_to_target_ctx(ctx),
            data,
        ),
        P::OpenAiResponsesToClaudeMessages => to_claude(
            gc::openai_responses_to_claude_messages::stream_event,
            ctx,
            data,
        ),
        P::OpenAiResponsesToGeminiGenerateContent => to_plain(
            gc::openai_responses_to_gemini_generate_content::stream_event,
            ctx,
            data,
        ),
        P::OpenAiResponsesToOpenAiChat => {
            to_plain(gc::openai_responses_to_openai_chat::stream_event, ctx, data)
        }
        other => Err(not_wired(other)),
    }
}

/// Decode + convert one stream event on the typed path (no `Value` legs).
fn run_stream<S: DeserializeOwned, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<T, TransformError> {
    let input: S = serde_json::from_str(data).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode stream event: {e}"),
    })?;
    f(input, ctx)
}

fn encode<T: Serialize>(event: Option<String>, out: &T) -> Result<StreamEventOut, TransformError> {
    let data = serde_json::to_string(out).map_err(|e| TransformError::Serialization {
        reason: e.to_string(),
    })?;
    Ok(StreamEventOut::Encoded { event, data })
}

/// Inbound wire is chat/gemini: data-only frames, no SSE event name.
fn to_plain<S: DeserializeOwned, T: Serialize>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<StreamEventOut, TransformError> {
    encode(None, &run_stream(f, ctx, data)?)
}

/// Inbound wire is Claude Messages: named SSE events.
fn to_claude<S: DeserializeOwned>(
    f: impl Fn(S, &TransformContext) -> Result<claude::StreamEvent, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<StreamEventOut, TransformError> {
    let out = run_stream(f, ctx, data)?;
    encode(out.event_name().map(str::to_owned), &out)
}

/// Inbound wire is Responses (HTTP or WebSocket): hand the typed event to the
/// caller's aggregation state machine.
fn to_responses<S: DeserializeOwned>(
    f: impl Fn(S, &TransformContext) -> Result<openai::ResponseStreamEvent, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<StreamEventOut, TransformError> {
    Ok(StreamEventOut::Responses(Box::new(run_stream(
        f, ctx, data,
    )?)))
}

fn stream_via_openai_responses<S: DeserializeOwned>(
    f: impl Fn(S, &TransformContext) -> Result<openai::ResponseStreamEvent, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<StreamEventOut, TransformError> {
    to_responses(f, &source_to_responses_ctx(ctx), data)
}

fn responses_key(ctx: &TransformContext, source: bool) -> OperationKey {
    OperationKey::content_generation(
        if source {
            ctx.source.operation
        } else {
            ctx.target.operation
        },
        ContentGenerationKind::OpenAiResponses,
    )
}

fn source_to_responses_ctx(ctx: &TransformContext) -> TransformContext {
    TransformContext::new(ctx.source, responses_key(ctx, true))
}

fn responses_to_target_ctx(ctx: &TransformContext) -> TransformContext {
    TransformContext::new(responses_key(ctx, false), ctx.target)
}

fn request_via_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run(f, &source_to_responses_ctx(ctx), body)
}

fn request_from_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run(f, &responses_to_target_ctx(ctx), body)
}

fn response_via_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run(f, &source_to_responses_ctx(ctx), body)
}

fn response_from_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run(f, &responses_to_target_ctx(ctx), body)
}
