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
        fanout::{
            ChatViaClaudeFanout, ChatViaResponsesFanout, FanoutOptions, FanoutProgress,
            FanoutTarget, GeminiViaClaudeFanout, GeminiViaResponsesFanout,
        },
        gemini_responses::{GeminiReturnFacts, GeminiViaResponses, ResponsesViaGemini},
        stream::{
            ChatViaGeminiStreamFacts, ClaudeViaGeminiStreamFacts, GeminiViaClaudeStreamFacts,
            ResponsesViaClaudeStreamFacts, ResponsesViaGeminiStreamFacts, StreamSettings,
            StreamStart, StreamTarget,
            event::EventLimits,
            reader::SourceFraming,
            synthesize::{CompleteResponse, GenerationStreamOutcome, synthesize},
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
        identity::{IdNamespace, IdentityTarget, TargetIdPolicy},
    },
    wire::{claude::generate_content as c, gemini as g, openai::chat as h, openai::responses as r},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
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

pub(super) fn decode<T: DeserializeOwned>(
    body: &[u8],
    limits: CodecLimits,
) -> Result<T, TransformError> {
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

pub(super) fn namespace() -> IdNamespace {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    IdNamespace(bytes)
}

fn auto() -> r::input::ToolChoice {
    r::input::ToolChoice::Mode(r::input::ToolChoiceMode::Auto)
}

pub(super) fn responses_settings(
    input: &r::GenerateContentRequestBody,
) -> (bool, r::input::ToolChoice) {
    (
        input.parallel_tool_calls.flatten().unwrap_or(true),
        input.tool_choice.clone().unwrap_or_else(auto),
    )
}

/// Responses client over a Chat upstream: the response context echoes the
/// client's effective controls.
pub(super) fn responses_over_chat_context(
    input: &r::GenerateContentRequestBody,
) -> chat_responses::stream::ChatToResponsesContext {
    let (parallel_tool_calls, tool_choice) = responses_settings(input);
    chat_responses::ResponsesResponseContext {
        request: input.clone(),
        effective_parallel_tool_calls: parallel_tool_calls,
        effective_tool_choice: tool_choice,
        usage: Default::default(),
        effective_prompt_cache_options: None,
    }
    .into()
}

/// Claude reports thinking tokens only when extended thinking ran. A client
/// that asked for no reasoning had none: zero is a fact, not a guess.
pub(super) fn reasoning_tokens_fact(input: &r::GenerateContentRequestBody) -> Option<i64> {
    input
        .reasoning
        .as_ref()
        .and_then(Option::as_ref)
        .is_none()
        .then_some(0)
}

pub(super) fn responses_over_claude_facts(
    input: &r::GenerateContentRequestBody,
    created: i64,
) -> ResponsesViaClaudeStreamFacts {
    let (parallel_tool_calls, tool_choice) = responses_settings(input);
    ResponsesViaClaudeStreamFacts {
        request: Default::default(),
        response: claude_responses::ClaudeResponseContext {
            request: input.clone(),
            effective_parallel_tool_calls: parallel_tool_calls,
            effective_tool_choice: tool_choice,
            usage: claude_responses::ResponsesUsageFacts {
                reasoning_tokens: reasoning_tokens_fact(input),
                ..Default::default()
            },
            created_at: created,
            effective_prompt_cache_options: None,
        }
        .into(),
    }
}

pub(super) fn responses_over_gemini_facts(
    input: &r::GenerateContentRequestBody,
    created: i64,
    model: &str,
) -> ResponsesViaGeminiStreamFacts {
    let (parallel_tool_calls, tool_choice) = responses_settings(input);
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
            actual_model: Some(model.to_owned()),
            final_thinking_tokens: None,
        },
    }
}

/// The gateway adds no event-count or structure-count budget.
const MAX_STREAM_EVENTS: usize = usize::MAX;
const MAX_STREAM_ITEMS: usize = usize::MAX;
const MAX_STREAM_TOOLS: usize = usize::MAX;
const MAX_STREAM_PARTS: usize = usize::MAX;
const MAX_STREAM_CHOICES: usize = usize::MAX;

pub(super) fn stream_settings(
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
pub(super) struct OwnedState<C> {
    pub store: ProtocolState<C>,
    pub scope: StateScope,
    pub target: IdentityTarget,
    pub conversation_key: String,
    pub expires_at: SystemTime,
    pub max_records: usize,
}

impl<C: BatchConnectionTrait + Send + Sync> OwnedState<C> {
    pub(super) fn capture(
        call: &Call<'_, C>,
        state: &GenerationStateAccess<'_, ProtocolState<C>>,
    ) -> Self {
        Self {
            store: call.state_store.clone(),
            scope: call.state_scope.clone(),
            target: state.target.clone(),
            conversation_key: state.conversation_key.clone(),
            expires_at: state.expires_at,
            max_records: state.max_records,
        }
    }

    pub(super) fn access(&self) -> GenerationStateAccess<'_, ProtocolState<C>> {
        GenerationStateAccess {
            store: &self.store,
            scope: &self.scope,
            target: self.target.clone(),
            conversation_key: self.conversation_key.clone(),
            expires_at: self.expires_at,
            now: SystemTime::UNIX_EPOCH
                + std::time::Duration::from_millis(crate::api::lifecycle::now_ms().max(0) as u64),
            max_records: self.max_records,
        }
    }
}

pub(super) fn rejected(response: WireResponse<gproxy_protocol::connection::Bytes>) -> Converted {
    Converted::Rejected(WireResponse {
        status: response.status,
        headers: response.headers,
        body: HttpBody::Bytes(response.body),
    })
}

pub(super) fn transport(error: TransformError) -> TransportError {
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
    if call.synthesize {
        let client = call.client.dialect;
        let settings = stream_settings(call.limits, client, call.request.query);
        let completion = Completion::Synthesize {
            framing: settings.client_framing,
            events: settings.events,
            include_usage: client != Dialect::OpenAiChat || chat_includes_usage(call.body()),
        };
        return if client == call.target {
            Box::pin(invoke_native_complete(call, completion)).await
        } else {
            Box::pin(invoke_complete(call, completion)).await
        };
    }
    Box::pin(invoke_stream(call)).await
}

async fn invoke_stream<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let client = call.client.dialect;
    let target = call.target;
    let upstream = call.upstream;
    let body = call.body();
    let limits = call.limits;
    let settings = stream_settings(limits, client, call.request.query);
    if target == Dialect::OpenAiResponsesWebSocket {
        return super::responses_ws::over_websocket(call, settings).await;
    }
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
    let created = call.now_ms.div_euclid(1000);
    let model = state.target.model.clone();

    if candidate_count(client, body) >= 2 {
        let fanout_target = FanoutTarget {
            endpoint: super::generate_endpoint(target, &model, true)?,
            options: fanout_options(client),
        };
        let client_framing = settings.client_framing;
        // Children start lazily on each poll, so the driven stream owns a
        // copy of the attempt-bound upstream. A child rejected after the head
        // went out surfaces as a transport error on the client stream.
        macro_rules! fan {
            ($pair:ty, $input:ty $(, |$index:ident| $ctx:expr)?) => {{
                let input: $input = decode(body, limits)?;
                Box::pin(async move {
                    let fanout = <$pair>::prepare_stream(
                        input,
                        fanout_target,
                        $(|$index: usize| $ctx,)?
                        settings,
                        state,
                    )
                    .await?;
                    let owned = OwnedState::capture(call, state);
                    let upstream = upstream.clone();
                    let stream: ByteStream = Box::pin(futures_util::stream::unfold(
                        Some((fanout, owned, upstream)),
                        move |slot| async move {
                            let (mut fanout, owned, upstream) = slot?;
                            let next = fanout.next(&upstream, &key, &owned.access()).await;
                            match next {
                                Ok(Some(chunk)) => {
                                    Some((Ok(chunk.bytes), Some((fanout, owned, upstream))))
                                }
                                Ok(None) => None,
                                Err(error) => Some((Err(transport(error)), None)),
                            }
                        },
                    ));
                    Ok(Converted::Stream(WireResponse {
                        status: http::StatusCode::OK,
                        headers: stream_headers(client_framing),
                        body: stream,
                    }))
                })
                .await
            }};
        }
        match (client, target) {
            (Dialect::OpenAiChat, Dialect::Claude) => {
                return fan!(ChatViaClaudeFanout, h::GenerateContentRequestBody, |_i| {
                    claude_chat::stream::ClaudeToChatContext { created }
                });
            }
            (Dialect::OpenAiChat, Dialect::OpenAi) => {
                return fan!(ChatViaResponsesFanout, h::GenerateContentRequestBody);
            }
            (Dialect::Gemini, Dialect::Claude) => {
                return fan!(GeminiViaClaudeFanout, g::GenerateContentRequestBody, |_i| {
                    GeminiViaClaudeStreamFacts {
                        max_tokens: None,
                        response: claude_gemini::stream::ClaudeToGeminiContext::default(),
                    }
                });
            }
            (Dialect::Gemini, Dialect::OpenAi) => {
                return fan!(
                    GeminiViaResponsesFanout,
                    g::GenerateContentRequestBody,
                    |_i| { gemini_responses::stream::ResponsesToGeminiContext::default() }
                );
            }
            _ => {}
        }
    }

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
                responses_over_chat_context(&input)
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
                responses_over_claude_facts(&input, created)
            })
        }
        (Dialect::Gemini, Dialect::OpenAi) => {
            run!(GeminiViaResponses, g::GenerateContentRequestBody, |input| {
                gemini_responses::stream::ResponsesToGeminiContext::default()
            })
        }
        (Dialect::OpenAi, Dialect::Gemini) => {
            run!(ResponsesViaGemini, r::GenerateContentRequestBody, |input| {
                responses_over_gemini_facts(&input, created, &model)
            })
        }
        (client, target) => Err(TransformError::unsupported(
            "generate.stream",
            format!("no streaming conversion from {client:?} to {target:?}"),
        )),
    }
}

/// Peek the requested candidate count; Chat `n` and Gemini
/// `generationConfig.candidateCount`. Anything else is one candidate.
fn candidate_count(client: Dialect, body: &[u8]) -> i64 {
    #[derive(Deserialize)]
    struct Chat {
        n: Option<i64>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Gemini {
        generation_config: Option<GeminiConfig>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GeminiConfig {
        candidate_count: Option<i64>,
    }
    match client {
        Dialect::OpenAiChat => serde_json::from_slice::<Chat>(body)
            .ok()
            .and_then(|chat| chat.n),
        Dialect::Gemini => serde_json::from_slice::<Gemini>(body)
            .ok()
            .and_then(|gemini| gemini.generation_config)
            .and_then(|config| config.candidate_count),
        _ => None,
    }
    .unwrap_or(1)
}

/// Chat `stream_options.include_usage`; only Chat clients opt into a usage
/// chunk on a synthesized stream.
fn chat_includes_usage(body: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Chat {
        stream_options: Option<StreamOptions>,
    }
    #[derive(Deserialize)]
    struct StreamOptions {
        include_usage: Option<bool>,
    }
    serde_json::from_slice::<Chat>(body)
        .ok()
        .and_then(|chat| chat.stream_options)
        .and_then(|options| options.include_usage)
        == Some(true)
}

/// Candidate fanout keeps every child journaled; this bounds the journal.
const MAX_CANDIDATES: usize = usize::MAX;

fn fanout_options(client: Dialect) -> FanoutOptions {
    FanoutOptions {
        namespace: namespace(),
        max_children: MAX_CANDIDATES,
        response_policy: TargetIdPolicy::new(client),
    }
}

pub(super) fn stream_headers(framing: SourceFraming) -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static(match framing {
            SourceFraming::Sse => "text/event-stream",
            _ => "application/json",
        }),
    );
    headers
}

fn json_headers() -> http::HeaderMap {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    headers
}

/// A complete client DTO that can also be replayed as its native stream.
trait SynthesizedClient: CompleteResponse {
    fn strip_usage(&mut self) {}
}
impl SynthesizedClient for c::GenerateContentResponseBody {}
impl SynthesizedClient for g::GenerateContentResponseBody {}
impl SynthesizedClient for r::GenerateContentResponseBody {}
impl SynthesizedClient for h::GenerateContentResponseBody {
    fn strip_usage(&mut self) {
        self.usage = None;
    }
}

/// How a complete upstream result is handed to the client.
#[derive(Clone, Copy)]
enum Completion {
    /// Encode the client DTO once.
    Buffered,
    /// Replay the client DTO as its native stream lifecycle.
    Synthesize {
        framing: SourceFraming,
        events: EventLimits,
        include_usage: bool,
    },
}

fn complete<Cl: SynthesizedClient>(
    mut outcome: GenerationOutcome<Cl>,
    completion: Completion,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    match completion {
        Completion::Buffered => finish(outcome, limits),
        Completion::Synthesize {
            framing,
            events,
            include_usage,
        } => {
            if !include_usage && let GenerationOutcome::Success { response, .. } = &mut outcome {
                response.body.strip_usage();
            }
            Ok(
                match synthesize(outcome, namespace(), framing, limits, events)? {
                    GenerationStreamOutcome::Success { response, .. } => match response.body {
                        HttpBody::Stream(stream) => Converted::Stream(WireResponse {
                            status: response.status,
                            headers: response.headers,
                            body: stream,
                        }),
                        body @ HttpBody::Bytes(_) => Converted::Success(WireResponse {
                            status: response.status,
                            headers: response.headers,
                            body,
                        }),
                    },
                    GenerationStreamOutcome::Rejected(raw) => rejected(raw),
                },
            )
        }
    }
}

/// One buffered generation for `client -> target`.
pub(crate) async fn buffered<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    Box::pin(invoke_complete(call, Completion::Buffered)).await
}

// Keep native synthesis out of the multi-protocol conversion dispatch frame.
async fn invoke_native_complete<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    completion: Completion,
) -> Result<Converted, TransformError> {
    let target = call.target;
    let upstream = call.upstream;
    let body = call.body();
    let limits = call.limits;
    let key = OperationKey {
        operation: Operation::GenerateContent,
        dialect: target,
    };
    let endpoint = super::generate_endpoint(target, call.model()?, false)?;
    let mut value: serde_json::Value = decode(body, limits)?;
    if target != Dialect::Gemini {
        let object = value
            .as_object_mut()
            .ok_or_else(|| TransformError::shape("request", "expected an object"))?;
        object.insert("stream".into(), serde_json::json!(false));
        object.remove("stream_options");
        object.insert("model".into(), serde_json::json!(call.model()?));
    }
    macro_rules! native {
        ($input:ty, $output:ty) => {
            Box::pin(async move {
                let input: $input = serde_json::from_value(value)
                    .map_err(|e| TransformError::shape("request", e.to_string()))?;
                let request = gproxy_protocol::WireRequest {
                    method: http::Method::POST,
                    path: endpoint.path.clone(),
                    query: endpoint.query.clone(),
                    headers: {
                        let mut headers = call.request.headers.clone();
                        headers.insert(
                            http::header::ACCEPT,
                            http::HeaderValue::from_static("application/json"),
                        );
                        headers
                    },
                    body: input,
                };
                match Box::pin(gproxy_protocol::adapt::invoke_json::<_, _, $output>(
                    upstream, &key, request, limits,
                ))
                .await?
                {
                    gproxy_protocol::adapt::JsonInvocation::Rejected(response) => {
                        Ok(Converted::Rejected(response))
                    }
                    gproxy_protocol::adapt::JsonInvocation::Success(response) => complete(
                        GenerationOutcome::Success {
                            response,
                            report: Default::default(),
                        },
                        completion,
                        limits,
                    ),
                }
            })
            .await
        };
    }
    return match target {
        Dialect::OpenAi => native!(
            r::GenerateContentRequestBody,
            r::GenerateContentResponseBody
        ),
        Dialect::OpenAiChat => native!(
            h::GenerateContentRequestBody,
            h::GenerateContentResponseBody
        ),
        Dialect::Claude => native!(
            c::GenerateContentRequestBody,
            c::GenerateContentResponseBody
        ),
        Dialect::Gemini => native!(
            g::GenerateContentRequestBody,
            g::GenerateContentResponseBody
        ),
        _ => Err(TransformError::unsupported(
            "route",
            "no buffered WebSocket synthesis",
        )),
    };
}

/// One complete generation for `client -> target`: decode the client's
/// request, prepare the directed edge with continuation state, POST once
/// through the attempt-bound upstream, and hand the client DTO back either
/// encoded once or replayed as its native stream. Chat `n` / Gemini
/// `candidateCount` of two or more against a single-result upstream fan out
/// into journaled child calls.
async fn invoke_complete<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
    completion: Completion,
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
    if client == target {
        return Box::pin(invoke_native_complete(call, completion)).await;
    }
    let identities = GenerationIdentity::new(namespace(), namespace(), client, target)?;
    let created = call.now_ms.div_euclid(1000);
    let synthesizing = matches!(completion, Completion::Synthesize { .. });

    if candidate_count(client, body) >= 2 {
        let fanout_target = || FanoutTarget {
            endpoint: endpoint.clone(),
            options: fanout_options(client),
        };
        macro_rules! fan {
            ($pair:ty, $input:ty, [$($extra:expr),*], $facts:expr) => {{
                if synthesizing {
                    return Err(TransformError::unsupported(
                        "generate.synthesis",
                        "multi-candidate synthesis is not supported",
                    ));
                }
                let input: $input = decode(body, limits)?;
                let fanout_target = fanout_target();
                return Box::pin(async move {
                    let mut prepared =
                        <$pair>::prepare(input, fanout_target, state, limits $(, $extra)*)
                            .await?;
                    let mut progress = FanoutProgress::default();
                    let converted = prepared
                        .invoke(upstream, &key, limits, state, &mut progress, $facts)
                        .await?;
                    let body = encode_json(&converted.value, limits).map_err(codec)?;
                    Ok(Converted::Success(WireResponse {
                        status: http::StatusCode::OK,
                        headers: json_headers(),
                        body: HttpBody::Bytes(body),
                    }))
                })
                .await;
            }};
        }
        match (client, target) {
            (Dialect::OpenAiChat, Dialect::Claude) => fan!(
                ChatViaClaudeFanout,
                h::GenerateContentRequestBody,
                [],
                |_, _: &c::GenerateContentResponseBody| Ok(claude_chat::ResponseSupplement {
                    created_unix_seconds: Some(created),
                })
            ),
            (Dialect::OpenAiChat, Dialect::OpenAi) => fan!(
                ChatViaResponsesFanout,
                h::GenerateContentRequestBody,
                [],
                |_, _: &r::GenerateContentResponseBody| Ok(())
            ),
            (Dialect::Gemini, Dialect::Claude) => fan!(
                GeminiViaClaudeFanout,
                g::GenerateContentRequestBody,
                [None],
                |_, native: &c::GenerateContentResponseBody| Ok(
                    claude_gemini::ClaudeGeminiUsageFacts {
                        cache_creation_input_tokens: native
                            .usage
                            .cache_creation_input_tokens
                            .flatten(),
                        cache_read_input_tokens: native.usage.cache_read_input_tokens.flatten(),
                        thinking_tokens: None,
                    }
                )
            ),
            (Dialect::Gemini, Dialect::OpenAi) => fan!(
                GeminiViaResponsesFanout,
                g::GenerateContentRequestBody,
                [],
                |_, _: &r::GenerateContentResponseBody| Ok(
                    gemini_responses::GeminiReplayContext::default()
                )
            ),
            // Chat <-> Gemini carry their native count; other clients have none.
            _ => {}
        }
    }

    macro_rules! run {
        ($pair:ty, $input:ty, [$($extra:expr),*], [$($synth_extra:expr),*], $facts:expr) => {{
            let input: $input = decode(body, limits)?;
            // Boxed per pair to keep the dispatch frame small.
            Box::pin(async move {
                let mut prepared = if synthesizing {
                    <$pair>::prepare_for_stream_synthesis(
                        input, endpoint, identities, state $(, $synth_extra)*
                    ).await?
                } else {
                    <$pair>::prepare_with_state(input, endpoint, identities, state $(, $extra)*)
                        .await?
                };
                let mut progress = GenerationProgress::default();
                let outcome = prepared
                    .invoke(upstream, &key, limits, state, &mut progress, $facts)
                    .await?;
                complete(outcome, completion, limits)
            })
            .await
        }};
    }
    match (client, target) {
        (Dialect::OpenAiChat, Dialect::Claude) => run!(
            ChatViaClaude,
            h::GenerateContentRequestBody,
            [],
            [],
            |_: &c::GenerateContentResponseBody| Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::Claude, Dialect::OpenAiChat) => run!(
            ClaudeViaChat,
            c::GenerateContentRequestBody,
            [],
            [],
            |_: &h::GenerateContentResponseBody| Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::OpenAiChat, Dialect::Gemini) => run!(
            ChatViaGemini,
            h::GenerateContentRequestBody,
            [&Default::default()],
            [&Default::default()],
            |_: &g::GenerateContentResponseBody| Ok(gemini_chat::GeminiChatResponseSupplement {
                created_unix_seconds: Some(created),
            })
        ),
        (Dialect::Gemini, Dialect::OpenAiChat) => run!(
            GeminiViaChat,
            g::GenerateContentRequestBody,
            [],
            [],
            |_: &h::GenerateContentResponseBody| Ok(())
        ),
        (Dialect::OpenAiChat, Dialect::OpenAi) => run!(
            ChatViaResponses,
            h::GenerateContentRequestBody,
            [],
            [],
            |_: &r::GenerateContentResponseBody| Ok(())
        ),
        (Dialect::OpenAi, Dialect::OpenAiChat) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            run!(
                ResponsesViaChat,
                r::GenerateContentRequestBody,
                [],
                [],
                move |_| Ok(ChatReturnFacts {
                    parallel_tool_calls,
                    tool_choice,
                    prompt_cache_options: None,
                    usage: Default::default(),
                })
            )
        }
        (Dialect::Claude, Dialect::Gemini) => run!(
            ClaudeViaGemini,
            c::GenerateContentRequestBody,
            [Default::default()],
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
            [],
            |_: &r::GenerateContentResponseBody| Ok(
                claude_responses::ClaudeRequestContext::default()
            )
        ),
        (Dialect::OpenAi, Dialect::Claude) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            let reasoning_tokens = reasoning_tokens_fact(&input);
            run!(
                ResponsesViaClaude,
                r::GenerateContentRequestBody,
                [Default::default()],
                [Default::default()],
                move |native: &c::GenerateContentResponseBody| Ok(ClaudeReturnFacts {
                    parallel_tool_calls,
                    tool_choice: tool_choice.clone(),
                    prompt_cache_options: None,
                    usage: claude_responses::ResponsesUsageFacts {
                        cache_write_tokens: native.usage.cache_creation_input_tokens.flatten(),
                        cached_tokens: native.usage.cache_read_input_tokens.flatten(),
                        reasoning_tokens,
                    },
                    created_at: created,
                })
            )
        }
        (Dialect::Gemini, Dialect::OpenAi) => run!(
            GeminiViaResponses,
            g::GenerateContentRequestBody,
            [],
            [],
            |_: &r::GenerateContentResponseBody| Ok(
                gemini_responses::GeminiReplayContext::default()
            )
        ),
        (Dialect::OpenAi, Dialect::Gemini) => {
            let input: r::GenerateContentRequestBody = decode(body, limits)?;
            let (parallel_tool_calls, tool_choice) = responses_settings(&input);
            run!(
                ResponsesViaGemini,
                r::GenerateContentRequestBody,
                [Default::default()],
                [Default::default()],
                move |native: &g::GenerateContentResponseBody| Ok(GeminiReturnFacts {
                    parallel_tool_calls,
                    tool_choice: tool_choice.clone(),
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
            )
        }
        (client, target) => Err(TransformError::unsupported(
            "generate",
            format!("no conversion from {client:?} to {target:?}"),
        )),
    }
}
