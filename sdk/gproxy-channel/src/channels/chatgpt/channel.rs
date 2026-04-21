//! `Channel` trait implementation for the ChatGPT web channel.

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::Instrument;

use super::image::{
    ImagePointer, build_openai_images_response, download_image_b64, extract_image_pointers,
    poll_conversation_for_images,
};
use super::image_edit::{parse_edit_body, upload_image_to_chatgpt, UploadResult};
use super::request_builder::{build_conversation_body, resolve_model};
use super::sentinel::{self, SentinelTokens};
use super::session::{
    OAI_CLIENT_VERSION, TurnContext, shared_fallback_client, stash_turn, standard_headers,
    take_turn,
};
use super::sse_to_openai::SseToOpenAi;

use crate::channel::{Channel, ChannelCredential, ChannelSettings, CommonChannelSettings};
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
    /// When `true`, every `/f/conversation` request is sent with
    /// `history_and_training_disabled: true`, so the turn is excluded
    /// from the user's ChatGPT history and from model training
    /// (equivalent to the "Temporary chat" toggle in the chatgpt.com UI).
    ///
    /// Defaults to `true` — gproxy assumes proxied traffic should not
    /// pollute the human user's personal history. Set to `false` if you
    /// want turns to appear in the normal chatgpt.com sidebar.
    #[serde(default = "default_temporary_chat")]
    pub temporary_chat: bool,
    #[serde(flatten)]
    pub common: CommonChannelSettings,
}

impl Default for ChatGptSettings {
    fn default() -> Self {
        Self {
            base_url: default_chatgpt_base_url(),
            temporary_chat: default_temporary_chat(),
            common: CommonChannelSettings::default(),
        }
    }
}

fn default_chatgpt_base_url() -> String {
    CHATGPT_BASE_URL.to_string()
}

fn default_temporary_chat() -> bool {
    true
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
            // Streaming route: declared as a same-src-dst `TransformTo`
            // so the engine builds a stream transformer. That transformer
            // uses `IdentityConverter` for the actual chat.completion.chunk
            // shape but drives our `normalize_response` per JSON chunk,
            // which is where we reshape ChatGPT's `/f/conversation` SSE v1
            // deltas into proper OpenAI chunks.
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
            ),
            pass(OperationFamily::CreateImage, ProtocolKind::OpenAi),
            pass(OperationFamily::CreateImageEdit, ProtocolKind::OpenAi),
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

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
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

        let is_image = matches!(
            request.route.operation,
            OperationFamily::CreateImage | OperationFamily::StreamCreateImage
        );
        let is_image_edit = matches!(
            request.route.operation,
            OperationFamily::CreateImageEdit | OperationFamily::StreamCreateImageEdit
        );

        // Image edit needs a side trip to the upload API before we can
        // build the `/f/conversation` body. Everything else parses the
        // body as JSON directly.
        let (chat_body, upload): (Value, Option<UploadResult>) = if is_image_edit {
            let parsed = parse_edit_body(&request.body).map_err(|e| {
                UpstreamError::Channel(format!("chatgpt: parse image-edit body: {e}"))
            })?;
            let client = shared_fallback_client()?;
            // prepare_request is sync but runs inside tokio; block on the
            // async upload so the rest of the request can be built
            // deterministically.
            let upload_res = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    super::session::warmup_fallback(&client, &credential.access_token)
                        .await
                        .ok();
                    upload_image_to_chatgpt(&client, &credential.access_token, &parsed).await
                })
            })?;
            let chat_body = serde_json::json!({
                "messages": [{"role": "user", "content": parsed.prompt}],
            });
            (chat_body, Some(upload_res))
        } else if is_image {
            // CreateImage: OpenAI body has `prompt`. Adapt to chat-like shape.
            let parsed: Value = serde_json::from_slice(&request.body).map_err(|e| {
                UpstreamError::Channel(format!("chatgpt: parse request body: {e}"))
            })?;
            let prompt = parsed
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (
                serde_json::json!({
                    "messages": [{"role": "user", "content": prompt}],
                }),
                None,
            )
        } else {
            let parsed: Value = serde_json::from_slice(&request.body).map_err(|e| {
                UpstreamError::Channel(format!("chatgpt: parse request body: {e}"))
            })?;
            (parsed, None)
        };

        let openai_body: Value = chat_body.clone();
        let model = resolve_model(
            request
                .model
                .as_deref()
                .or_else(|| openai_body.get("model").and_then(|v| v.as_str()))
                .unwrap_or(""),
        );
        let mut body_map = build_conversation_body(&chat_body, &model, settings.temporary_chat);

        // For image edit: rewrite the user message's content to multimodal
        // and attach the uploaded file as an asset pointer + attachment.
        if let Some(upload) = upload.as_ref() {
            attach_uploaded_image(&mut body_map, upload);
        }

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
        // for `CreateImage` / `CreateImageEdit` routes. Includes a
        // cookie-enabled fallback client suitable for the file-download API.
        if (is_image || is_image_edit)
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

        // Image generation/edit routes: pull out file-service pointers,
        // download them via the stashed fallback client, and return a
        // standard OpenAI `images.response` body.
        if matches!(
            request.route.operation,
            OperationFamily::CreateImage
                | OperationFamily::StreamCreateImage
                | OperationFamily::CreateImageEdit
                | OperationFamily::StreamCreateImageEdit
        ) {
            let turn_id = request
                .headers
                .get("x-chatgpt-turn-id")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            return normalize_image_response(&body, turn_id.as_deref()).unwrap_or(body);
        }

        let model = request
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5".to_string());

        // Two modes:
        //
        // * **Non-stream** (`GenerateContent`): the engine calls us once
        //   with the full SSE body. We decode it in one shot and emit a
        //   `chat.completion` JSON.
        //
        // * **Stream** (`StreamGenerateContent`): the engine calls us
        //   per JSON chunk via the stream transformer's `normalizer`
        //   closure. Input is one ChatGPT SSE data payload, output is
        //   one `chat.completion.chunk` JSON (or empty to skip).
        //   Per-turn state (channel map / accumulated text) lives in
        //   the shared `turn_stream_state` keyed by `x-chatgpt-turn-id`.
        let streaming = matches!(
            request.route.operation,
            OperationFamily::StreamGenerateContent
        );
        if streaming {
            return reshape_stream_chunk(request, &body, &model);
        }

        // Non-stream path: full buffered SSE → chat.completion JSON.
        let chunks = super::sse_to_openai::collect_all(&model, &body);
        if chunks.is_empty() {
            return body;
        }
        // Aggregate into a single chat.completion object.
        let mut content = String::new();
        for chunk in &chunks {
            if let Some(s) = chunk
                .choices
                .first()
                .and_then(|c| c.delta.get("content"))
                .and_then(|v| v.as_str())
            {
                content.push_str(s);
            }
        }
        let msg_id = chunks
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

/// Mutate a `/f/conversation` body in place to attach an uploaded image
/// onto the single user message:
/// * `content.content_type` becomes `multimodal_text`
/// * `content.parts[0]` becomes an `image_asset_pointer` object
/// * The prompt text is appended as `parts[1]`
/// * `metadata.attachments[0]` describes the uploaded file
fn attach_uploaded_image(
    body: &mut serde_json::Map<String, Value>,
    upload: &UploadResult,
) {
    let messages = match body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        Some(m) if !m.is_empty() => m,
        _ => return,
    };
    let user_msg = &mut messages[0];
    let prompt_text = user_msg
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let asset = serde_json::json!({
        "content_type": "image_asset_pointer",
        "asset_pointer": format!("sediment://{}", upload.file_id),
        "size_bytes": upload.size_bytes,
        "width": upload.width,
        "height": upload.height,
    });

    if let Some(content) = user_msg.get_mut("content")
        && let Some(obj) = content.as_object_mut()
    {
        obj.insert("content_type".to_string(), Value::String("multimodal_text".into()));
        let mut parts = Vec::with_capacity(2);
        parts.push(asset.clone());
        if !prompt_text.is_empty() {
            parts.push(Value::String(prompt_text));
        }
        obj.insert("parts".to_string(), Value::Array(parts));
    }

    if let Some(metadata) = user_msg.get_mut("metadata")
        && let Some(md) = metadata.as_object_mut()
    {
        md.insert(
            "attachments".to_string(),
            Value::Array(vec![serde_json::json!({
                "id": upload.file_id,
                "size": upload.size_bytes,
                "name": upload.filename,
                "mime_type": upload.mime_type,
                "width": upload.width,
                "height": upload.height,
                "source": "library",
                "is_big_paste": false,
            })]),
        );
    }
}

/// Per-turn stream state. One entry per in-flight stream request,
/// keyed by `x-chatgpt-turn-id`. Populated lazily on the first
/// `normalize_response` call and kept alive across chunks.
fn stream_state_cache() -> &'static dashmap::DashMap<String, std::sync::Mutex<SseToOpenAi>> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<dashmap::DashMap<String, std::sync::Mutex<SseToOpenAi>>> = OnceLock::new();
    CACHE.get_or_init(dashmap::DashMap::new)
}

/// Apply a single ChatGPT SSE JSON chunk against the turn's state machine
/// and return one `chat.completion.chunk` JSON (or empty Vec for events
/// that don't emit a downstream chunk).
///
/// The stream transformer's decoder has already split the raw SSE bytes
/// into one JSON payload per `data: ...` line. Here we:
///   1. Parse the payload into an [`sse_v1::Event`]
///   2. Look up (or create) the per-turn [`SseToOpenAi`] state
///   3. Push the event, get at most one OpenAI chunk back
///   4. Serialize it as JSON (no SSE framing — that's added by the
///      transformer's encoder)
fn reshape_stream_chunk(request: &PreparedRequest, chunk: &[u8], model: &str) -> Vec<u8> {
    let Some(turn_id) = request
        .headers
        .get("x-chatgpt-turn-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
    else {
        return Vec::new();
    };

    // Parse the JSON payload. Non-object payloads (the version banner
    // `"v1"`, `[DONE]` residues, etc.) are treated as no-op events.
    let value: Value = match serde_json::from_slice(chunk) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let event = if value.is_object() {
        // Build an `sse_v1::Event` by hand rather than going through
        // `SseDecoder` — decoder-level framing has already been done.
        if let Some(kind) = value.get("type").and_then(|v| v.as_str())
            && value.get("v").is_none()
            && value.get("p").is_none()
        {
            super::sse_v1::Event::Typed {
                kind: kind.to_string(),
                raw: value,
            }
        } else {
            super::sse_v1::Event::Delta(parse_delta_from_value(&value))
        }
    } else {
        return Vec::new();
    };

    let cache = stream_state_cache();
    // Take-out pattern: remove the mutex, use it, put it back. Avoids
    // deadlocking against `cache.remove(...)` / `cache.retain(...)` which
    // need the same shard lock that `.entry(...)` holds for its lifetime.
    let state = cache
        .remove(&turn_id)
        .map(|(_, m)| m)
        .unwrap_or_else(|| std::sync::Mutex::new(SseToOpenAi::with_model(model)));
    let (rendered, keep) = {
        let mut guard = state.lock().expect("poisoned stream state");
        let was_finished = guard.finished();
        let output = guard.on_event(event);
        let bytes = output
            .as_ref()
            .map(|c| serde_json::to_vec(c).unwrap_or_default())
            .unwrap_or_default();
        let keep = !(!was_finished && guard.finished());
        (bytes, keep)
    };
    if keep {
        cache.insert(turn_id, state);
    }

    // Evict everything if the map grows past the soft cap. Cheap, very
    // rare, and avoids long O(N) scans mid-request.
    if cache.len() > 4096 {
        cache.clear();
    }

    rendered
}

/// Mirror of [`super::sse_v1::SseDecoder::parse_delta`] but operating on an
/// already-parsed [`Value`] (since the engine's stream decoder pre-parses
/// JSON). Kept private to avoid exposing a duplicate API.
fn parse_delta_from_value(v: &Value) -> super::sse_v1::Delta {
    use super::sse_v1::{Delta, PatchKind, PatchOp};

    let channel = v.get("c").and_then(|c| c.as_u64());
    let op = v.get("o").and_then(|x| x.as_str()).unwrap_or("");

    if op == "patch"
        && let Some(arr) = v.get("v").and_then(|x| x.as_array())
    {
        return Delta {
            channel,
            patches: arr.iter().filter_map(parse_one_patch_value).collect(),
        };
    }

    if v.get("o").is_none() && v.get("p").is_none() {
        if let Some(arr) = v.get("v").and_then(|x| x.as_array()) {
            return Delta {
                channel,
                patches: arr.iter().filter_map(parse_one_patch_value).collect(),
            };
        }
        if let Some(obj_value) = v.get("v")
            && obj_value.is_object()
        {
            return Delta {
                channel,
                patches: vec![PatchOp {
                    path: String::new(),
                    op: PatchKind::Add,
                    value: obj_value.clone(),
                }],
            };
        }
    }

    Delta {
        channel,
        patches: vec![PatchOp {
            path: v.get("p").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            op: PatchKind::parse(op),
            value: v.get("v").cloned().unwrap_or(Value::Null),
        }],
    }
}

fn parse_one_patch_value(v: &Value) -> Option<super::sse_v1::PatchOp> {
    let obj = v.as_object()?;
    Some(super::sse_v1::PatchOp {
        path: obj
            .get("p")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        op: super::sse_v1::PatchKind::parse(
            obj.get("o").and_then(|x| x.as_str()).unwrap_or(""),
        ),
        value: obj.get("v").cloned().unwrap_or(Value::Null),
    })
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
