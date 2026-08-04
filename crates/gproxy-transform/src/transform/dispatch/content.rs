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

pub(super) struct ContentStreamConverter {
    pair: TransformPair,
    ctx: TransformContext,
    state: ContentStreamState,
}

enum ContentStreamState {
    Stateless,
    ClaudeToResponses(gc::claude_messages_to_openai_responses::StreamTransform),
    GeminiToResponses(gc::gemini_generate_content_to_openai_responses::StreamTransform),
    ChatToResponses(gc::openai_chat_to_openai_responses::StreamTransform),
    ResponsesToClaude(gc::openai_responses_to_claude_messages::StreamTransform),
    ResponsesToGemini(gc::openai_responses_to_gemini_generate_content::StreamTransform),
    ResponsesToChat(gc::openai_responses_to_openai_chat::StreamTransform),
    ChatToClaude(gc::openai_chat_to_claude_messages::StreamTransform),
    GeminiToClaude(gc::gemini_generate_content_to_claude_messages::StreamTransform),
}

impl ContentStreamConverter {
    pub(super) fn new(pair: TransformPair, ctx: TransformContext) -> Result<Self, TransformError> {
        use TransformPair as P;
        let state = match pair {
            P::ClaudeMessagesToOpenAiResponses | P::ClaudeMessagesToOpenAiResponsesWebSocket => {
                ContentStreamState::ClaudeToResponses(
                    gc::claude_messages_to_openai_responses::StreamTransform::default(),
                )
            }
            P::GeminiGenerateContentToOpenAiResponses
            | P::GeminiGenerateContentToOpenAiResponsesWebSocket => {
                ContentStreamState::GeminiToResponses(
                    gc::gemini_generate_content_to_openai_responses::StreamTransform,
                )
            }
            P::OpenAiChatToOpenAiResponses | P::OpenAiChatToOpenAiResponsesWebSocket => {
                ContentStreamState::ChatToResponses(
                    gc::openai_chat_to_openai_responses::StreamTransform::default(),
                )
            }
            P::OpenAiResponsesToClaudeMessages | P::OpenAiResponsesWebSocketToClaudeMessages => {
                ContentStreamState::ResponsesToClaude(
                    gc::openai_responses_to_claude_messages::StreamTransform,
                )
            }
            P::OpenAiResponsesToGeminiGenerateContent
            | P::OpenAiResponsesWebSocketToGeminiGenerateContent => {
                ContentStreamState::ResponsesToGemini(
                    gc::openai_responses_to_gemini_generate_content::StreamTransform::default(),
                )
            }
            P::OpenAiResponsesToOpenAiChat | P::OpenAiResponsesWebSocketToOpenAiChat => {
                ContentStreamState::ResponsesToChat(
                    gc::openai_responses_to_openai_chat::StreamTransform::default(),
                )
            }
            P::OpenAiChatToClaudeMessages => ContentStreamState::ChatToClaude(
                gc::openai_chat_to_claude_messages::StreamTransform,
            ),
            P::GeminiGenerateContentToClaudeMessages => ContentStreamState::GeminiToClaude(
                gc::gemini_generate_content_to_claude_messages::StreamTransform,
            ),
            other if is_content(other) => ContentStreamState::Stateless,
            other => return Err(not_wired(other)),
        };
        Ok(Self { pair, ctx, state })
    }

    pub(super) fn push(&mut self, data: &str) -> Result<Vec<StreamEventOut>, TransformError> {
        use TransformPair as P;
        match (&mut self.state, self.pair) {
            (ContentStreamState::Stateless, P::ClaudeMessagesToGeminiGenerateContent) => {
                to_plain_one(
                    gc::claude_messages_to_gemini_generate_content::stream_event,
                    &self.ctx,
                    data,
                )
            }
            (ContentStreamState::Stateless, P::ClaudeMessagesToOpenAiChat) => to_plain_one(
                gc::claude_messages_to_openai_chat::stream_event,
                &self.ctx,
                data,
            ),
            (ContentStreamState::ClaudeToResponses(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::ClaudeMessagesToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.push(input, &ctx)?)
            }
            (
                ContentStreamState::GeminiToClaude(state),
                P::GeminiGenerateContentToClaudeMessages,
            ) => {
                let input = decode_stream(data)?;
                to_claude_many(state.push(input, &self.ctx)?)
            }
            (ContentStreamState::Stateless, P::GeminiGenerateContentToOpenAiChat) => to_plain_one(
                gc::gemini_generate_content_to_openai_chat::stream_event,
                &self.ctx,
                data,
            ),
            (ContentStreamState::GeminiToResponses(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::GeminiGenerateContentToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.push(input, &ctx)?)
            }
            (ContentStreamState::ChatToClaude(state), P::OpenAiChatToClaudeMessages) => {
                let input = decode_stream(data)?;
                to_claude_many(state.push(input, &self.ctx)?)
            }
            (ContentStreamState::Stateless, P::OpenAiChatToGeminiGenerateContent) => to_plain_one(
                gc::openai_chat_to_gemini_generate_content::stream_event,
                &self.ctx,
                data,
            ),
            (ContentStreamState::ChatToResponses(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiChatToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.push(input, &ctx)?)
            }
            (
                ContentStreamState::Stateless,
                P::OpenAiResponsesToOpenAiResponsesWebSocket
                | P::OpenAiResponsesWebSocketToOpenAiResponses,
            ) => to_responses_many(vec![decode_stream(data)?]),
            (ContentStreamState::ResponsesToChat(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToOpenAiChat {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_plain_many(state.push(input, &ctx)?)
            }
            (ContentStreamState::ResponsesToClaude(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToClaudeMessages {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_claude_many(state.push(input, &ctx)?)
            }
            (ContentStreamState::ResponsesToGemini(state), _) => {
                let input = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToGeminiGenerateContent {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_plain_many(state.push(input, &ctx)?)
            }
            _ => Err(not_wired(self.pair)),
        }
    }

    pub(super) fn finish(&mut self) -> Result<Vec<StreamEventOut>, TransformError> {
        use TransformPair as P;
        match &mut self.state {
            ContentStreamState::Stateless => Ok(Vec::new()),
            ContentStreamState::ClaudeToResponses(state) => {
                let ctx = if self.pair == P::ClaudeMessagesToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.finish(&ctx)?)
            }
            ContentStreamState::GeminiToResponses(state) => {
                let ctx = if self.pair == P::GeminiGenerateContentToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.finish(&ctx)?)
            }
            ContentStreamState::ChatToResponses(state) => {
                let ctx = if self.pair == P::OpenAiChatToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_responses_many(state.finish(&ctx)?)
            }
            ContentStreamState::ResponsesToClaude(state) => {
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToClaudeMessages {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_claude_many(state.finish(&ctx)?)
            }
            ContentStreamState::ResponsesToGemini(state) => {
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToGeminiGenerateContent {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_plain_many(state.finish(&ctx)?)
            }
            ContentStreamState::ResponsesToChat(state) => {
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToOpenAiChat {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                to_plain_many(state.finish(&ctx)?)
            }
            ContentStreamState::ChatToClaude(state) => to_claude_many(state.finish(&self.ctx)?),
            ContentStreamState::GeminiToClaude(state) => to_claude_many(state.finish(&self.ctx)?),
        }
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

fn decode_stream<S: DeserializeOwned>(data: &str) -> Result<S, TransformError> {
    serde_json::from_str(data).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode stream event: {e}"),
    })
}

fn encode<T: Serialize>(event: Option<String>, out: &T) -> Result<StreamEventOut, TransformError> {
    let data = serde_json::to_string(out).map_err(|e| TransformError::Serialization {
        reason: e.to_string(),
    })?;
    Ok(StreamEventOut::Encoded { event, data })
}

/// Inbound wire is chat/gemini: data-only frames, no SSE event name.
fn to_plain_one<S: DeserializeOwned, T: Serialize>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<Vec<StreamEventOut>, TransformError> {
    Ok(vec![encode(None, &run_stream(f, ctx, data)?)?])
}

fn to_plain_many<T: Serialize>(events: Vec<T>) -> Result<Vec<StreamEventOut>, TransformError> {
    events
        .into_iter()
        .map(|event| encode(None, &event))
        .collect()
}

fn to_claude_many(events: Vec<claude::StreamEvent>) -> Result<Vec<StreamEventOut>, TransformError> {
    events
        .into_iter()
        .map(|event| encode(event.event_name().map(str::to_owned), &event))
        .collect()
}

fn to_responses_many(
    events: Vec<openai::ResponseStreamEvent>,
) -> Result<Vec<StreamEventOut>, TransformError> {
    Ok(events
        .into_iter()
        .map(|event| StreamEventOut::Responses(Box::new(event)))
        .collect())
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
