//! ChatGPT consumer web-backend channel — proxies `chatgpt.com/backend-api/f/conversation`
//! using a browser **session** credential (no OAuth). Anti-bot machinery (sentinel
//! chat-requirements + proof-of-work + `__cf_bm` warmup) is carried in the credential
//! secret and refreshed lazily. Inbound pivots on OpenAI Chat Completions; the SSE-v1
//! ⇄ OpenAI translation happens in-channel ([`prepare`](ChatGptChannel::prepare) builds
//! the `/f/conversation` body; [`ChatGptStreamDecoder`] decodes the SSE-v1 multi-channel
//! patch stream back into OpenAI `chat.completion.chunk` SSE, surfacing reasoning as
//! `reasoning_content`). Image gen/edit is a later phase. See
//! `docs/superpowers/specs/2026-06-21-chatgpt-channel-design.md`.

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::{
    Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, Disposition, PrepareCtx,
    PreparedRequest,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

mod auth;
mod browser_date;
mod conduit;
mod config;
mod cookie;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod headers;
mod image_download;
mod image_upload;
mod images;
mod models;
mod pow;
mod project;
mod request_builder;
mod request_hints;
mod sentinel;
mod sse;
mod sse_to_openai;

use sse::SseDecoder;
use sse_to_openai::SseToOpenAi;

/// Default chatgpt.com web-backend origin.
const DEFAULT_BASE_URL: &str = "https://chatgpt.com";

pub struct ChatGptChannel;

impl ChatGptChannel {
    pub const ID: &'static str = "chatgpt";
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for ChatGptChannel {
    fn id(&self) -> &'static str {
        "chatgpt"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        // The upstream is ALWAYS a stream (chatgpt SSE-v1); every content op
        // pivots on OpenAiChatCompletions and routes to a streaming upstream.
        // Non-stream clients aggregate via the existing pipeline (same model as
        // codex). The chatgpt SSE-v1 ⇄ openai-chat translation is in-channel —
        // the transform layer only ever sees openai-chat in / openai-chat-SSE out.
        let mut routes = vec![
            // chatgpt.com has no `/v1/models` endpoint, but it DOES serve a live
            // picker at `/backend-api/models/gpts`; ListModels fetches it online
            // and `shape_response` reshapes it (bundled catalogue is only a
            // parse-failure fallback). GetModel is served locally from the
            // exposed models the online pull seeded.
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            local(GetModel, pv(P::OpenAi)),
            local(GetModel, pv(P::Claude)),
            local(GetModel, pv(P::Gemini)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            // Image gen/edit: a channel-driven multi-step Custom exchange in
            // `prepare` (conversation → poll → download → images.response).
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
        ];
        routes.extend(responses_ws_to(cg(OpenAiChatCompletions)));
        routes
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    /// ListModels reshapes the live `/backend-api/models/gpts` picker into the
    /// OpenAI model-list shape (bundled catalogue as a parse-failure fallback).
    fn shape_response(&self, body: Bytes, ctx: &crate::channel::ShapeCtx) -> Bytes {
        match ctx.op.operation {
            crate::protocol::Operation::ListModels => models::reshape_model_list(&body),
            _ => body,
        }
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let base = ctx
            .provider_settings
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_BASE_URL);

        // Image gen/edit is a channel-driven multi-step exchange: the pipeline
        // injects the resolved client and `images::run` orchestrates the
        // conversation → poll → download → `images.response` collapse. Detect
        // it by path before the models/conversation branches.
        if ctx.path.contains("/images/generations") || ctx.path.contains("/images/edits") {
            let is_edit = ctx.path.contains("/images/edits");
            let secret = ctx.secret.clone();
            let inbound = ctx.body.clone();
            let base = base.to_string();
            let model = request_builder::resolve_model(ctx.upstream_model_id);
            return Ok(PreparedRequest::custom(Box::new(move |client| {
                Box::pin(
                    async move { images::run(client, secret, base, model, inbound, is_edit).await },
                )
            })));
        }

        // Model-list requests hit the live `/backend-api/models/gpts` picker;
        // everything else builds a `/f/conversation` body. GetModel is `local`,
        // so only ListModels reaches prepare with a models path.
        if ctx.path.contains("models") {
            let url = format!("{base}/backend-api/models/gpts");
            let mut req = http::Request::get(url)
                .body(Bytes::new())
                .map_err(|e| ChannelError::Build(format!("chatgpt models request build: {e}")))?;
            auth::apply_request_headers(&mut req, ctx.secret)?;
            return Ok(PreparedRequest::new(req));
        }

        // Session mode: `normal` | `temporary` (default) | `project`. Back-compat:
        // a missing `mode` falls back to the legacy `temporary_chat` bool
        // (`true`→`temporary`, `false`→`normal`).
        let mode = ctx
            .provider_settings
            .get("mode")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                match ctx
                    .provider_settings
                    .get("temporary_chat")
                    .and_then(Value::as_bool)
                {
                    Some(false) => "normal".to_string(),
                    _ => "temporary".to_string(),
                }
            });
        let temporary_chat = mode == "temporary";
        // `project` mode opens conversations inside a ChatGPT project (so they
        // show up grouped in the user's project history). The `g-p-…` id is
        // resolved (find-or-create) + cached; read the cache synchronously here.
        // If not yet resolved, the async path below resolves it (this turn
        // degrades to a normal persistent chat until then — never an error).
        let project_name: Option<String> = (mode == "project").then(|| {
            ctx.provider_settings
                .get("project_name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("gproxy")
                .to_string()
        });
        let project_id: Option<String> = project_name
            .as_deref()
            .and_then(|n| project::cached(ctx.secret, base, n));
        let needs_project_resolve = project_name.is_some() && project_id.is_none();

        let parsed: Value = serde_json::from_slice(&ctx.body)
            .map_err(|e| ChannelError::Build(format!("chatgpt request body parse: {e}")))?;
        let resolved = request_builder::resolve_model(ctx.upstream_model_id);

        // Thinking / pro / deep-research turns answer via `stream_handoff`: the
        // `/f/conversation` response is a stub and the turn streams over the
        // conduit WebSocket. Run them as a streaming Custom (POST → detect
        // handoff → conduit stream) so the chain-of-thought + report stream out
        // incrementally. Other models stream inline on the Direct path below.
        if is_handoff_model(&resolved) || wants_deep_research(&parsed) || needs_project_resolve {
            let secret = ctx.secret.clone();
            let base = base.to_string();
            let inbound = ctx.body.clone();
            let model = resolved.clone();
            return Ok(PreparedRequest::custom_stream(Box::new(move |client| {
                Box::pin(async move {
                    chat_via_conduit(
                        client,
                        secret,
                        base,
                        model,
                        inbound,
                        temporary_chat,
                        project_name,
                    )
                    .await
                })
            })));
        }

        let body_map = request_builder::build_conversation_body(
            &parsed,
            &resolved,
            temporary_chat,
            project_id.as_deref(),
        );
        let body = serde_json::to_vec(&body_map)
            .map_err(|e| ChannelError::Build(format!("chatgpt request body serialize: {e}")))?;

        let url = format!("{base}/backend-api/f/conversation");
        let mut req = http::Request::post(url)
            .body(Bytes::from(body))
            .map_err(|e| ChannelError::Build(format!("chatgpt request build: {e}")))?;
        auth::apply_request_headers(&mut req, ctx.secret)?;
        Ok(PreparedRequest::new(req))
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(ChatGptStreamDecoder::new()))
    }

    /// A Cloudflare or auth rejection (`401`/`403`, or a `cf-mitigated`
    /// challenge header on any status) means the browser-session credential +
    /// anti-bot tokens are dead — fail it over for a refresh. Everything else
    /// uses the generic HTTP-status mapping.
    fn classify(
        &self,
        status: http::StatusCode,
        headers: &http::HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        if status == http::StatusCode::UNAUTHORIZED
            || status == http::StatusCode::FORBIDDEN
            || headers.contains_key("cf-mitigated")
        {
            return Disposition::AuthDead;
        }
        Disposition::from_http(status, headers)
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret, crate::util::time::unix_now_ms() as i64)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for ChatGptChannel {
    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        cookie::exchange(client, cookie).await
    }
}

/// Per-stream decoder: chatgpt SSE-v1 multi-channel patch stream → OpenAI
/// `chat.completion.chunk` SSE bytes. The transform layer treats the output as
/// ordinary openai-chat SSE.
struct ChatGptStreamDecoder {
    decoder: SseDecoder,
    converter: SseToOpenAi,
}

impl ChatGptStreamDecoder {
    fn new() -> Self {
        Self {
            decoder: SseDecoder::new(),
            converter: SseToOpenAi::new(),
        }
    }

    fn drain_events(&mut self, out: &mut Vec<u8>) {
        while let Some(event) = self.decoder.next_event() {
            if let Some(chunk) = self.converter.on_event(event) {
                push_chunk(out, &chunk);
            }
        }
    }
}

impl ChannelStreamDecoder for ChatGptStreamDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.decoder.feed(chunk);
        let mut out = Vec::new();
        self.drain_events(&mut out);
        out
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.drain_events(&mut out);
        // Synthesize a stop if the upstream never sent `finished_successfully`.
        if !self.converter.finished() && self.converter.emitted_role() {
            let chunk = self.converter.on_event(sse::Event::Done);
            if let Some(chunk) = chunk {
                push_chunk(&mut out, &chunk);
            }
        }
        out.extend_from_slice(b"data: [DONE]\n\n");
        out
    }
}

/// Serialize one chunk as an OpenAI SSE `data:` frame.
fn push_chunk(out: &mut Vec<u8>, chunk: &sse_to_openai::OpenAiChunk) {
    if let Ok(json) = serde_json::to_string(chunk) {
        out.extend_from_slice(format!("data: {json}\n\n").as_bytes());
    }
}

/// Whether a model slug answers via `stream_handoff` (thinking / pro / o-series
/// reasoning) and therefore needs the conduit path. `supports_buffering:false`
/// does NOT keep these inline — they always hand off. Other models stream
/// inline. Heuristic on the slug; an unexpected handoff on a non-matching model
/// degrades to the stub (rare), and a non-handoff on a matching model is handled
/// (the conduit closure returns the inline body when no handoff is present).
fn is_handoff_model(slug: &str) -> bool {
    let s = slug.to_ascii_lowercase();
    s.contains("thinking")
        || s.contains("pro")
        || s.starts_with("o1")
        || s.starts_with("o3")
        || s.starts_with("o4")
}

/// Whether the request asks for deep research (the `deep_research` tool →
/// `connector:connector_openai_deep_research` hint). Deep research is NOT a
/// distinct model slug, so it must be detected from the body to route to the
/// conduit (it always hands off and streams `add-messages`).
fn wants_deep_research(parsed: &Value) -> bool {
    request_hints::extract_system_hints(parsed)
        .iter()
        .any(|h| h.contains("deep_research"))
}

/// Streaming Custom for handoff turns (thinking / pro / deep-research): POST
/// `/f/conversation`, and if the response is a `stream_handoff` stub, stream the
/// turn off the conduit WebSocket; otherwise stream back the inline response.
/// Yields SSE-v1 that the channel's [`ChatGptStreamDecoder`] decodes into OpenAI
/// chunks (chain-of-thought as `reasoning_content`, answer/report as `content`).
async fn chat_via_conduit(
    client: Arc<dyn UpstreamClient>,
    secret: Value,
    base: String,
    model: String,
    inbound: Bytes,
    temporary_chat: bool,
    project_name: Option<String>,
) -> Result<
    (
        http::StatusCode,
        http::HeaderMap,
        crate::http::client::RespStream,
    ),
    crate::http::client::ClientError,
> {
    use crate::http::client::ClientError;
    use futures_util::StreamExt;

    let parsed: Value = serde_json::from_slice(&inbound)
        .map_err(|e| ClientError::Transport(format!("chatgpt request body parse: {e}")))?;
    // `project` mode: resolve (find-or-create) the `g-p-…` id, caching it so
    // later turns read it synchronously in `prepare()`. A failure yields `None`
    // → this turn is a normal persistent chat (no error).
    let project_id = match &project_name {
        Some(name) => project::resolve(&client, &secret, &base, name).await,
        None => None,
    };
    let body_map = request_builder::build_conversation_body(
        &parsed,
        &model,
        temporary_chat,
        project_id.as_deref(),
    );
    let body = serde_json::to_vec(&body_map)
        .map_err(|e| ClientError::Transport(format!("chatgpt request serialize: {e}")))?;

    let url = format!("{base}/backend-api/f/conversation");
    let mut req = http::Request::post(url)
        .body(Bytes::from(body))
        .map_err(|e| ClientError::Transport(format!("chatgpt request build: {e}")))?;
    auth::apply_request_headers(&mut req, &secret)
        .map_err(|e| ClientError::Transport(e.to_string()))?;

    let resp = client.send(req).await?;
    let (parts, stub) = resp.into_parts();

    let sse_headers = || {
        let mut h = http::HeaderMap::new();
        h.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        h
    };

    // Non-success, or inline (no handoff) response → stream the body as one chunk.
    let one_chunk = |status: http::StatusCode, headers: http::HeaderMap, body: Bytes| {
        let st: crate::http::client::RespStream =
            futures_util::stream::once(async move { Ok(body) }).boxed();
        (status, headers, st)
    };

    if !parts.status.is_success() {
        return Ok(one_chunk(parts.status, parts.headers, stub));
    }
    match conduit::extract_handoff_turn(&stub) {
        Some(turn_id) => {
            let st = conduit::fetch_turn_stream(client, secret, base, turn_id)
                .await
                .map_err(ClientError::Transport)?;
            Ok((http::StatusCode::OK, sse_headers(), st))
        }
        None => Ok(one_chunk(http::StatusCode::OK, sse_headers(), stub)),
    }
}
