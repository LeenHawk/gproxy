//! Content-generation dispatch arms: the 12 pairs wired in M2, including
//! per-event stream conversion.

use serde_json::Value;

use super::{not_wired, run, run_ok, run_value};
use crate::protocol::{ContentGenerationKind, OperationKey};
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

pub(super) fn stream_event_value(
    pair: TransformPair,
    ctx: &TransformContext,
    event: Value,
) -> Result<Value, TransformError> {
    use TransformPair as P;
    match pair {
        P::ClaudeMessagesToGeminiGenerateContent => run_value(
            gc::claude_messages_to_gemini_generate_content::stream_event,
            ctx,
            event,
        ),
        P::ClaudeMessagesToOpenAiChat => {
            run_value(gc::claude_messages_to_openai_chat::stream_event, ctx, event)
        }
        P::ClaudeMessagesToOpenAiResponses => run_value(
            gc::claude_messages_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::GeminiGenerateContentToClaudeMessages => run_value(
            gc::gemini_generate_content_to_claude_messages::stream_event,
            ctx,
            event,
        ),
        P::GeminiGenerateContentToOpenAiChat => run_value(
            gc::gemini_generate_content_to_openai_chat::stream_event,
            ctx,
            event,
        ),
        P::GeminiGenerateContentToOpenAiResponses => run_value(
            gc::gemini_generate_content_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::OpenAiChatToClaudeMessages => {
            run_value(gc::openai_chat_to_claude_messages::stream_event, ctx, event)
        }
        P::OpenAiChatToGeminiGenerateContent => run_value(
            gc::openai_chat_to_gemini_generate_content::stream_event,
            ctx,
            event,
        ),
        P::OpenAiChatToOpenAiResponses => run_value(
            gc::openai_chat_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesToOpenAiResponsesWebSocket
        | P::OpenAiResponsesWebSocketToOpenAiResponses => {
            run_value(gc::openai_responses_websocket::identity_result, ctx, event)
        }
        P::OpenAiChatToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::openai_chat_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesWebSocketToOpenAiChat => stream_from_openai_responses(
            gc::openai_responses_to_openai_chat::stream_event,
            ctx,
            event,
        ),
        P::ClaudeMessagesToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::claude_messages_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesWebSocketToClaudeMessages => stream_from_openai_responses(
            gc::openai_responses_to_claude_messages::stream_event,
            ctx,
            event,
        ),
        P::GeminiGenerateContentToOpenAiResponsesWebSocket => stream_via_openai_responses(
            gc::gemini_generate_content_to_openai_responses::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesWebSocketToGeminiGenerateContent => stream_from_openai_responses(
            gc::openai_responses_to_gemini_generate_content::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesToClaudeMessages => run_value(
            gc::openai_responses_to_claude_messages::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesToGeminiGenerateContent => run_value(
            gc::openai_responses_to_gemini_generate_content::stream_event,
            ctx,
            event,
        ),
        P::OpenAiResponsesToOpenAiChat => run_value(
            gc::openai_responses_to_openai_chat::stream_event,
            ctx,
            event,
        ),
        other => Err(not_wired(other)),
    }
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

fn stream_via_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    event: Value,
) -> Result<Value, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run_value(f, &source_to_responses_ctx(ctx), event)
}

fn stream_from_openai_responses<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    event: Value,
) -> Result<Value, TransformError>
where
    S: serde::de::DeserializeOwned,
    T: serde::Serialize,
{
    run_value(f, &responses_to_target_ctx(ctx), event)
}
