//! Content-generation dispatch arms: the 12 pairs wired in M2, including
//! per-event stream conversion.

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{StreamEventOut, not_wired, run, run_ok};
use crate::protocol::{ContentGenerationKind, OperationKey, claude, gemini, openai};
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

pub(super) struct ContentStreamOutput {
    pub events: Vec<StreamEventOut>,
    pub terminal: bool,
}

impl ContentStreamOutput {
    fn new(events: Vec<StreamEventOut>, terminal: bool) -> Self {
        Self { events, terminal }
    }
}

/// Terminal state derived from the already-decoded source event. Keeping this
/// next to the typed dispatch prevents the SSE adapter from parsing every JSON
/// frame a second time just to inspect one field.
trait SourceStreamEvent {
    fn is_terminal(&self) -> bool;
}

impl SourceStreamEvent for claude::StreamEvent {
    fn is_terminal(&self) -> bool {
        matches!(self.event_name(), Some("message_stop" | "error"))
    }
}

impl SourceStreamEvent for openai::ResponseStreamEvent {
    fn is_terminal(&self) -> bool {
        matches!(
            self.event_name(),
            Some("response.completed" | "response.incomplete" | "response.failed" | "error")
        )
    }
}

impl SourceStreamEvent for openai::ChatCompletionChunk {
    fn is_terminal(&self) -> bool {
        // Chat streams terminate with the non-JSON `data: [DONE]` sentinel.
        false
    }
}

impl SourceStreamEvent for gemini::StreamGenerateContentChunk {
    fn is_terminal(&self) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.finish_reason.is_some())
            || self
                .prompt_feedback
                .as_ref()
                .is_some_and(|feedback| feedback.block_reason.is_some())
    }
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
    GeminiToChat(gc::gemini_generate_content_to_openai_chat::StreamTransform),
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
                    gc::gemini_generate_content_to_openai_responses::StreamTransform::default(),
                )
            }
            P::OpenAiChatToOpenAiResponses | P::OpenAiChatToOpenAiResponsesWebSocket => {
                ContentStreamState::ChatToResponses(
                    gc::openai_chat_to_openai_responses::StreamTransform::default(),
                )
            }
            P::OpenAiResponsesToClaudeMessages | P::OpenAiResponsesWebSocketToClaudeMessages => {
                ContentStreamState::ResponsesToClaude(
                    gc::openai_responses_to_claude_messages::StreamTransform::default(),
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
                gc::openai_chat_to_claude_messages::StreamTransform::default(),
            ),
            P::GeminiGenerateContentToClaudeMessages => ContentStreamState::GeminiToClaude(
                gc::gemini_generate_content_to_claude_messages::StreamTransform::default(),
            ),
            P::GeminiGenerateContentToOpenAiChat => ContentStreamState::GeminiToChat(
                gc::gemini_generate_content_to_openai_chat::StreamTransform::default(),
            ),
            other if is_content(other) => ContentStreamState::Stateless,
            other => return Err(not_wired(other)),
        };
        Ok(Self { pair, ctx, state })
    }

    pub(super) fn take_diagnostics(&self) -> Vec<crate::transform::TransformDiagnostic> {
        self.ctx.take_diagnostics()
    }

    pub(super) fn push(&mut self, data: &str) -> Result<ContentStreamOutput, TransformError> {
        let context = self.ctx.clone();
        context.scope(|| self.push_inner(data))
    }

    fn push_inner(&mut self, data: &str) -> Result<ContentStreamOutput, TransformError> {
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
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::ClaudeMessagesToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_responses_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            (
                ContentStreamState::GeminiToClaude(state),
                P::GeminiGenerateContentToClaudeMessages,
            ) => {
                let (input, terminal) = decode_stream(data)?;
                Ok(ContentStreamOutput::new(
                    to_claude_many(state.push(input, &self.ctx)?)?,
                    terminal,
                ))
            }
            (ContentStreamState::GeminiToChat(state), P::GeminiGenerateContentToOpenAiChat) => {
                let (input, terminal) = decode_stream(data)?;
                let event = state.push(input, &self.ctx)?;
                Ok(ContentStreamOutput::new(
                    vec![encode(None, &event)?],
                    terminal,
                ))
            }
            (ContentStreamState::GeminiToResponses(state), _) => {
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::GeminiGenerateContentToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_responses_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            (ContentStreamState::ChatToClaude(state), P::OpenAiChatToClaudeMessages) => {
                let (input, terminal) = decode_stream(data)?;
                Ok(ContentStreamOutput::new(
                    to_claude_many(state.push(input, &self.ctx)?)?,
                    terminal,
                ))
            }
            (ContentStreamState::Stateless, P::OpenAiChatToGeminiGenerateContent) => to_plain_one(
                gc::openai_chat_to_gemini_generate_content::stream_event,
                &self.ctx,
                data,
            ),
            (ContentStreamState::ChatToResponses(state), _) => {
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiChatToOpenAiResponsesWebSocket {
                    source_to_responses_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_responses_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            (
                ContentStreamState::Stateless,
                P::OpenAiResponsesToOpenAiResponsesWebSocket
                | P::OpenAiResponsesWebSocketToOpenAiResponses,
            ) => {
                let (input, terminal) = decode_stream(data)?;
                Ok(ContentStreamOutput::new(
                    to_responses_many(vec![input])?,
                    terminal,
                ))
            }
            (ContentStreamState::ResponsesToChat(state), _) => {
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToOpenAiChat {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_plain_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            (ContentStreamState::ResponsesToClaude(state), _) => {
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToClaudeMessages {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_claude_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            (ContentStreamState::ResponsesToGemini(state), _) => {
                let (input, terminal) = decode_stream(data)?;
                let ctx = if self.pair == P::OpenAiResponsesWebSocketToGeminiGenerateContent {
                    responses_to_target_ctx(&self.ctx)
                } else {
                    self.ctx.clone()
                };
                Ok(ContentStreamOutput::new(
                    to_plain_many(state.push(input, &ctx)?)?,
                    terminal,
                ))
            }
            _ => Err(not_wired(self.pair)),
        }
    }

    pub(super) fn finish(&mut self) -> Result<Vec<StreamEventOut>, TransformError> {
        let context = self.ctx.clone();
        context.scope(|| self.finish_inner())
    }

    fn finish_inner(&mut self) -> Result<Vec<StreamEventOut>, TransformError> {
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
            ContentStreamState::GeminiToChat(state) => to_plain_many(state.finish(&self.ctx)?),
        }
    }
}

/// Decode + convert one stream event on the typed path (no `Value` legs).
fn run_stream<S: DeserializeOwned + SourceStreamEvent, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<(T, bool), TransformError> {
    let input: S = serde_json::from_str(data).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode stream event: {e}"),
    })?;
    let terminal = input.is_terminal();
    Ok((f(input, ctx)?, terminal))
}

fn decode_stream<S: DeserializeOwned + SourceStreamEvent>(
    data: &str,
) -> Result<(S, bool), TransformError> {
    let input: S = serde_json::from_str(data).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode stream event: {e}"),
    })?;
    let terminal = input.is_terminal();
    Ok((input, terminal))
}

fn encode<T: Serialize>(event: Option<String>, out: &T) -> Result<StreamEventOut, TransformError> {
    let data = serde_json::to_string(out).map_err(|e| TransformError::Serialization {
        reason: e.to_string(),
    })?;
    Ok(StreamEventOut::Encoded { event, data })
}

/// Inbound wire is chat/gemini: data-only frames, no SSE event name.
fn to_plain_one<S: DeserializeOwned + SourceStreamEvent, T: Serialize>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    data: &str,
) -> Result<ContentStreamOutput, TransformError> {
    let (event, terminal) = run_stream(f, ctx, data)?;
    Ok(ContentStreamOutput::new(
        vec![encode(None, &event)?],
        terminal,
    ))
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
            ctx.source.operation()
        } else {
            ctx.target.operation()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_value(output: ContentStreamOutput) -> serde_json::Value {
        let event = output.events.into_iter().next().unwrap();
        let StreamEventOut::Encoded { data, .. } = event else {
            panic!("expected encoded chat event");
        };
        serde_json::from_str(&data).unwrap()
    }

    #[test]
    fn typed_source_events_report_terminal_state() {
        for raw in [
            r#"{"type":"message_stop"}"#,
            r#"{"type":"error","error":{"type":"server_error","message":"failed"}}"#,
        ] {
            let (_, terminal) =
                decode_stream::<claude::StreamEvent>(raw).expect("Claude terminal event");
            assert!(terminal, "{raw}");
        }

        let (_, terminal) = decode_stream::<claude::StreamEvent>(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
        )
        .expect("Claude content event");
        assert!(!terminal);

        for raw in [
            r#"{"type":"response.completed","response":{"id":"r1","created_at":0,"object":"response","output":[],"status":"completed"}}"#,
            r#"{"type":"response.incomplete","response":{"id":"r1","created_at":0,"object":"response","output":[],"status":"incomplete"}}"#,
            r#"{"type":"response.failed","response":{"id":"r1","created_at":0,"object":"response","output":[],"status":"failed"}}"#,
            r#"{"type":"error","code":"server_error","message":"failed","param":""}"#,
        ] {
            let (_, terminal) = decode_stream::<openai::ResponseStreamEvent>(raw)
                .expect("Responses terminal event");
            assert!(terminal, "{raw}");
        }

        let (_, terminal) = decode_stream::<gemini::StreamGenerateContentChunk>(
            r#"{"candidates":[{"index":0,"finishReason":"STOP"}]}"#,
        )
        .expect("Gemini terminal event");
        assert!(terminal);

        let (_, terminal) = decode_stream::<gemini::StreamGenerateContentChunk>(
            r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#,
        )
        .expect("Gemini blocked terminal event");
        assert!(terminal);

        let (_, terminal) = decode_stream::<openai::ChatCompletionChunk>(
            r#"{"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[]}"#,
        )
        .expect("Chat content event");
        assert!(!terminal);
    }

    #[test]
    fn unknown_typed_event_keeps_its_terminal_semantics() {
        let (event, terminal) =
            decode_stream::<claude::StreamEvent>(r#"{"type":"future_event","payload":1}"#)
                .expect("unknown Claude event");
        assert!(matches!(event, claude::StreamEvent::Unknown(_)));
        assert!(!terminal);

        let (event, terminal) =
            decode_stream::<claude::StreamEvent>(r#"{"type":"error","future_shape":true}"#)
                .expect("future Claude error event");
        assert!(matches!(event, claude::StreamEvent::Unknown(_)));
        assert!(terminal);
    }

    #[test]
    fn gemini_to_chat_dispatch_retains_parallel_tool_state_across_chunks() {
        let source = OperationKey::content_generation(
            crate::protocol::Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        );
        let target = OperationKey::content_generation(
            crate::protocol::Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let pair = crate::transform::resolve(source, target).unwrap();
        let mut converter =
            ContentStreamConverter::new(pair, TransformContext::new(source, target)).unwrap();

        let first = encoded_value(
            converter
                .push(
                    r#"{"responseId":"r1","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"call_1","name":"weather","args":{"city":"北京"}}}]}}]}"#,
                )
                .unwrap(),
        );
        let second = encoded_value(
            converter
                .push(
                    r#"{"responseId":"r1","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"call_2","name":"weather","args":{"city":"上海"}}}]}}]}"#,
                )
                .unwrap(),
        );
        let terminal = encoded_value(
            converter
                .push(
                    r#"{"responseId":"r1","modelVersion":"m","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[]}}]}"#,
                )
                .unwrap(),
        );

        assert_eq!(first["choices"][0]["delta"]["tool_calls"][0]["index"], 0);
        assert_eq!(second["choices"][0]["delta"]["tool_calls"][0]["index"], 1);
        assert_eq!(terminal["choices"][0]["finish_reason"], "tool_calls");
    }
}
