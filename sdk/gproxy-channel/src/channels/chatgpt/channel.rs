//! `Channel` trait implementation for the ChatGPT web channel.

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::Instrument;

use super::image::{
    ImagePointer, build_openai_images_response, download_image_b64, extract_image_pointers,
    poll_conversation_for_images,
};
use super::request_builder::{build_conversation_body, resolve_model};
use super::sentinel::{self, SentinelTokens};
use super::session::{
    OAI_CLIENT_VERSION, TurnContext, shared_fallback_client, stash_turn, standard_headers,
    take_turn,
};
use super::sse_to_openai::SseToOpenAi;
use super::sse_v1::SseDecoder;
use super::stream_reshaper::OpenAiChunkReshaper;

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, CommonChannelSettings, StreamReshaper,
};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

const CHATGPT_BASE_URL: &str = "https://chatgpt.com";
const CONVERSATION_PATH: &str = "/backend-api/f/conversation";
/// Refresh the sentinel token when it is within this many ms of expiring.
const SENTINEL_REFRESH_SKEW_MS: u64 = 60_000;

/// ChatGPT web channel.
pub struct ChatGptChannel;

impl ChatGptChannel {
    pub const ID: &'static str = "chatgpt";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGptSettings {
    #[serde(default = "default_chatgpt_base_url")]
    pub base_url: String,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

impl Default for ChatGptSettings {
    fn default() -> Self {
        Self {
            base_url: default_chatgpt_base_url(),
            common: CommonChannelSettings::default(),
        }
    }
}

fn default_chatgpt_base_url() -> String {
    CHATGPT_BASE_URL.to_string()
}

impl ChannelSettings for ChatGptSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

/// Credential for the ChatGPT web channel.
///
/// `access_token` is the JWT from `chatgpt.com/api/auth/session`.
/// The other fields are populated by [`Channel::refresh_credential`]
/// after running the sentinel dance, and consumed by
/// [`Channel::prepare_request`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatGptCredential {
    pub access_token: String,
    /// Value for `openai-sentinel-chat-requirements-token` header.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chat_req_token: String,
    /// Value for `openai-sentinel-proof-token` header (previous PoW answer).
    /// Used as a fallback if we cannot compute a fresh one. Normal path
    /// computes one in `prepare_request` each turn.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub proof_token: String,
    /// Unix millis expiry of `chat_req_token`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub chat_req_token_expires_at_ms: u64,
    /// Persona returned by the server (`chatgpt-paid`, `chatgpt-free`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<String>,
    /// Cached user-agent / device fingerprint. Not essential but kept in
    /// sync to avoid recomputing UUIDs on every request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

impl ChannelCredential for ChatGptCredential {
    fn apply_update(&mut self, update: &Value) -> bool {
        let mut changed = false;
        if let Some(tok) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = tok.to_string();
            changed = true;
        }
        if let Some(tok) = update.get("chat_req_token").and_then(|v| v.as_str()) {
            self.chat_req_token = tok.to_string();
            changed = true;
        }
        if let Some(exp) = update
            .get("chat_req_token_expires_at_ms")
            .and_then(|v| v.as_u64())
        {
            self.chat_req_token_expires_at_ms = exp;
            changed = true;
        }
        if let Some(tok) = update.get("proof_token").and_then(|v| v.as_str()) {
            self.proof_token = tok.to_string();
            changed = true;
        }
        changed
    }
}

impl Channel for ChatGptChannel {
    const ID: &'static str = Self::ID;
    type Settings = ChatGptSettings;
    type Credential = ChatGptCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut t = RoutingTable::new();
        // We speak OpenAI-chat-completion as the primary downstream
        // protocol. The engine's protocol transforms (Claude → openai,
        // Gemini → openai) take care of translating other shapes into
        // chat completions before they hit our channel.
        let pass = |op: OperationFamily, proto: ProtocolKind| {
            (RouteKey::new(op, proto), RouteImplementation::Passthrough)
        };
        let xform = |op: OperationFamily,
                     proto: ProtocolKind,
                     dst_op: OperationFamily,
                     dst_proto: ProtocolKind| {
            (
                RouteKey::new(op, proto),
                RouteImplementation::TransformTo {
                    destination: RouteKey::new(dst_op, dst_proto),
                },
            )
        };

        let routes: Vec<(RouteKey, RouteImplementation)> = vec![
            pass(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::CreateImage, ProtocolKind::OpenAi),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
        ];
        for (key, implementation) in routes {
            t.set(key, implementation);
        }
        t
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::Local
    }

    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        // Attach a per-turn trace id to both ends of the pipeline so
        // `prepare_request` (stash TurnContext) and `normalize_response`
        // (look it up) agree on a key. The engine passes the SAME
        // PreparedRequest object to both hooks, so stashing a header here
        // is the simplest way to thread state through.
        if !request.headers.contains_key("x-chatgpt-turn-id") {
            let id = uuid::Uuid::new_v4().to_string();
            if let Ok(v) = http::HeaderValue::from_str(&id) {
                request
                    .headers
                    .insert(http::HeaderName::from_static("x-chatgpt-turn-id"), v);
            }
        }
        Ok(request)
    }

    fn needs_spoof_client(&self, _credential: &Self::Credential) -> bool {
        // ChatGPT web absolutely requires the browser-impersonating (spoof)
        // client: the Cloudflare WAF in front of chatgpt.com matches the
        // TLS + H2 fingerprint, and the `__cf_bm` cookie issued on warmup
        // is bound to that fingerprint. The engine's spoof client is built
        // with `cookie_store(true)` (engine.rs), so Set-Cookie from our
        // warmup survives into the actual `/f/conversation` request.
        true
    }

    fn stream_reshaper(
        &self,
        request: &PreparedRequest,
    ) -> Option<Box<dyn StreamReshaper>> {
        // Only reshape streaming text responses. Image routes go through
        // normalize_response (they produce a single buffered JSON).
        if !matches!(
            request.route.operation,
            OperationFamily::StreamGenerateContent
        ) {
            return None;
        }
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5".to_string());
        Some(Box::new(OpenAiChunkReshaper::new(&model)))
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        _settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        if credential.access_token.is_empty() {
            return Err(UpstreamError::Channel(
                "chatgpt credential missing access_token".into(),
            ));
        }
        if credential.chat_req_token.is_empty() {
            return Err(UpstreamError::Channel(
                "chatgpt credential missing chat_req_token; refresh first".into(),
            ));
        }

        let openai_body: Value = serde_json::from_slice(&request.body).map_err(|e| {
            UpstreamError::Channel(format!("chatgpt: parse request body: {e}"))
        })?;
        let is_image = matches!(
            request.route.operation,
            OperationFamily::CreateImage | OperationFamily::StreamCreateImage
        );
        // For image requests, adapt the prompt-only body into a chat-like
        // shape so `build_conversation_body` can reuse the same path.
        let chat_body: Value = if is_image {
            let prompt = openai_body
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "messages": [{"role": "user", "content": prompt}],
            })
        } else {
            openai_body.clone()
        };
        let model = resolve_model(
            request
                .model
                .as_deref()
                .or_else(|| openai_body.get("model").and_then(|v| v.as_str()))
                .unwrap_or(""),
        );
        let body_map = build_conversation_body(&chat_body, &model);
        let body_bytes = serde_json::to_vec(&Value::Object(body_map)).map_err(|e| {
            UpstreamError::Channel(format!("chatgpt: serialize body: {e}"))
        })?;

        // Reuse the PoW answer we computed during finalize. The live
        // browser does the same: a single PoW is used both as the finalize
        // body's `proofofwork` field and as the `openai-sentinel-proof-token`
        // header on the subsequent `/f/conversation` call.
        let proof_token = credential.proof_token.clone();

        let url = format!("{}{}", CHATGPT_BASE_URL, CONVERSATION_PATH);
        let device_id = credential
            .device_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let turn_id = request
            .headers
            .get("x-chatgpt-turn-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let trace_id = uuid::Uuid::new_v4().to_string();

        // Stash turn context so normalize_response can fetch image bytes
        // for `CreateImage` routes. Includes a cookie-enabled fallback
        // client suitable for the file-download API.
        if is_image
            && let Ok(fallback_client) = shared_fallback_client()
        {
            stash_turn(
                turn_id.clone(),
                TurnContext {
                    access_token: credential.access_token.clone(),
                    chat_req_token: credential.chat_req_token.clone(),
                    device_id: device_id.clone(),
                    client: fallback_client,
                },
            );
        }

        let mut builder = http::Request::builder()
            .method(http::Method::POST)
            .uri(&url);

        // Standard headers.
        for (k, v) in std::convert::Into::<http::HeaderMap>::into(standard_headers(
            &credential.access_token,
        ))
        .iter()
        {
            builder = builder.header(k.clone(), v.clone());
        }
        builder = builder
            .header("accept", "text/event-stream")
            .header("oai-device-id", device_id)
            .header("oai-client-version", OAI_CLIENT_VERSION)
            .header(
                "openai-sentinel-chat-requirements-token",
                &credential.chat_req_token,
            )
            .header("openai-sentinel-proof-token", &proof_token)
            .header("x-oai-turn-trace-id", trace_id)
            .header("x-openai-target-path", CONVERSATION_PATH);

        // User-provided extra headers.
        for (k, v) in request.headers.iter() {
            builder = builder.header(k, v);
        }

        builder
            .body(body_bytes)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        if body.is_empty() {
            return body;
        }

        // Image generation route: pull out file-service pointers, download
        // them via the stashed fallback client, and return a standard
        // OpenAI `images.response` body.
        if matches!(
            request.route.operation,
            OperationFamily::CreateImage | OperationFamily::StreamCreateImage
        ) {
            let turn_id = request
                .headers
                .get("x-chatgpt-turn-id")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            return normalize_image_response(&body, turn_id.as_deref()).unwrap_or(body);
        }

        // We are only asked to normalize when buffered; streaming is
        // handled by the engine separately. For a buffered SSE body, we
        // parse the whole stream and re-emit as standard OpenAI
        // chat.completion.chunk SSE or chat.completion (non-stream)
        // depending on the caller's request.
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5".to_string());
        let mut decoder = SseDecoder::new();
        let mut converter = SseToOpenAi::with_model(&model);
        decoder.feed(&body);
        let mut openai_chunks = Vec::new();
        while let Some(event) = decoder.next_event() {
            openai_chunks.extend(converter.on_event(event));
        }
        if openai_chunks.is_empty() {
            return body;
        }

        let streaming = request.route.operation == OperationFamily::StreamGenerateContent
            || request.route.operation == OperationFamily::OpenAiResponseWebSocket;
        if streaming {
            let mut out = Vec::with_capacity(body.len());
            for chunk in &openai_chunks {
                out.extend_from_slice(b"data: ");
                out.extend_from_slice(
                    &serde_json::to_vec(chunk).unwrap_or_default(),
                );
                out.extend_from_slice(b"\n\n");
            }
            out.extend_from_slice(b"data: [DONE]\n\n");
            out
        } else {
            // Aggregate into a single chat.completion object.
            let content = converter.text().to_string();
            let msg_id = openai_chunks
                .first()
                .map(|c| c.id.clone())
                .unwrap_or_else(|| format!("chatcmpl-{}", uuid::Uuid::new_v4()));
            let response = serde_json::json!({
                "id": msg_id,
                "object": "chat.completion",
                "created": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop"
                }]
            });
            serde_json::to_vec(&response).unwrap_or(body)
        }
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401 | 403 => ResponseClassification::AuthDead,
            429 => {
                let retry_after = headers
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|secs| secs * 1000);
                ResponseClassification::RateLimited {
                    retry_after_ms: retry_after,
                }
            }
            500..=599 => ResponseClassification::TransientError,
            _ => {
                if let Some(cf) = headers.get("cf-mitigated") {
                    tracing::warn!(
                        cf_mitigated = ?cf,
                        body_prefix = %String::from_utf8_lossy(&body[..body.len().min(200)]),
                        "chatgpt blocked by cloudflare challenge"
                    );
                }
                ResponseClassification::PermanentError
            }
        }
    }

    fn needs_refresh(&self, credential: &Self::Credential) -> bool {
        if credential.chat_req_token.is_empty() {
            return true;
        }
        sentinel::is_expired(
            credential.chat_req_token_expires_at_ms,
            SENTINEL_REFRESH_SKEW_MS,
        )
    }

    fn refresh_credential<'a>(
        &'a self,
        client: &'a wreq::Client,
        credential: &'a mut Self::Credential,
    ) -> impl Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        let span = tracing::info_span!("refresh_credential", channel = "chatgpt");
        async move {
            if credential.access_token.is_empty() {
                return Err(UpstreamError::Channel(
                    "chatgpt refresh: access_token is empty".into(),
                ));
            }
            let tokens: SentinelTokens =
                sentinel::run_sentinel(client, &credential.access_token).await?;
            credential.chat_req_token = tokens.chat_req_token;
            credential.proof_token = tokens.proof_token;
            credential.chat_req_token_expires_at_ms = tokens.chat_req_token_expires_at_ms;
            credential.persona = tokens.persona.or(credential.persona.clone());
            if credential.device_id.is_none() {
                credential.device_id = Some(uuid::Uuid::new_v4().to_string());
            }
            Ok(true)
        }
        .instrument(span)
    }
}

fn chatgpt_routing_table() -> RoutingTable {
    ChatGptChannel.routing_table()
}

/// Parse an image-generation SSE body and return an OpenAI
/// `images.response` JSON body. Uses the turn-scoped [`TurnContext`]
/// previously stashed by `prepare_request` to download each pointer's
/// image bytes and base64-encode them.
///
/// On any failure, returns `None` so the caller can fall back to the raw
/// body (preserving diagnostic info in logs).
fn normalize_image_response(body: &[u8], turn_id: Option<&str>) -> Option<Vec<u8>> {
    let (mut pointers, conversation_id) = extract_image_pointers(body);

    // Lift the turn context out of the stash. If we have no context we
    // cannot authenticate the download; bail to raw body.
    let ctx = turn_id.and_then(take_turn)?;

    let results: Vec<(String, String)> = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            // Warmup cookies on the fallback client the first time we use
            // it; inexpensive after the first successful call. Uses its
            // own LAST timestamp so it runs once per fallback client,
            // regardless of the engine client's warmup history.
            let _ = super::session::warmup_fallback(&ctx.client, &ctx.access_token).await;

            // Image generation on chatgpt.com is ASYNC: the initial SSE
            // only emits a "Processing image" tool message and returns
            // BEFORE the file-service pointer appears. Poll the
            // conversation endpoint until the real pointers show up.
            if pointers.is_empty()
                && let Some(cid) = conversation_id.as_deref()
            {
                match poll_conversation_for_images(
                    &ctx.client,
                    &ctx.access_token,
                    &ctx.device_id,
                    cid,
                    180,
                )
                .await
                {
                    Ok(ptrs) => pointers = ptrs,
                    Err(e) => {
                        tracing::warn!(error = %e, "chatgpt image poll failed");
                        return Vec::<(String, String)>::new();
                    }
                }
            }

            let mut out = Vec::new();
            let with_cid: Vec<ImagePointer> = pointers
                .into_iter()
                .map(|mut p| {
                    if p.conversation_id.is_empty()
                        && let Some(cid) = conversation_id.as_deref()
                    {
                        p.conversation_id = cid.to_string();
                    }
                    p
                })
                .collect();
            for ptr in &with_cid {
                match download_image_b64(
                    &ctx.client,
                    &ctx.access_token,
                    &ctx.device_id,
                    ptr,
                )
                .await
                {
                    Ok(b64) => out.push((b64, String::new())),
                    Err(e) => tracing::warn!(
                        error = %e,
                        pointer = %ptr.id,
                        "chatgpt image download failed"
                    ),
                }
            }
            out
        })
    });

    if results.is_empty() {
        return None;
    }
    let wrapped = build_openai_images_response(results);
    serde_json::to_vec(&wrapped).ok()
}

inventory::submit! { ChannelRegistration::new(ChatGptChannel::ID, chatgpt_routing_table) }
