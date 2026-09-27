//! Grok Build: an xAI account granted through the `grok-shell` CLI's device
//! login and used against the Grok Build chat proxy.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/grokbuild` on
//! `main`) and the CLI's own request builder. This is not the `xai` channel:
//! that one is an API key against `api.x.ai`, and an OAuth account cannot use
//! it for generation at all.
//!
//! * **Two origins.** Generation, the catalogue and billing live on
//!   `cli-chat-proxy.grok.com/v1`; xAI's own media paths — images, `/tts`,
//!   `/stt`, video — live on `api.x.ai/v1`, which the same token reaches
//!   (`request`).
//! * **Request.** The proxy takes OpenAI Chat and Responses, but a narrower
//!   Responses body than OpenAI: no continuation id, no bookkeeping fields, no
//!   hosted tool search, no encrypted reasoning it did not produce (`shape`).
//!   Speech is renamed into xAI's own fields, as in `xai`.
//! * **Identity.** `grok-shell` announces itself with a fixed
//!   `x-grok-client-*` block, an `x-xai-token-auth` marker, its user agent,
//!   the account's `sub` in `x-grok-user-id` and the body's
//!   `prompt_cache_key` in `x-grok-conv-id` (`auth`).
//! * **Credential.** A device login at `auth.x.ai` and a form-encoded
//!   refresh; the account subject and email come out of the id token at login
//!   and live in the credential's metadata (`oauth`).
//! * **Quota.** `/billing?format=credits` on the chat proxy (`quota`).
//!
//! Session identity: `design/session-identity.md` reads Grok Build's session
//! from `x-grok-session-id`, and says `x-grok-conv-id` is *not* it — the CLI
//! regenerates that one on some auxiliary calls. So `x-grok-session-id` is
//! neither set nor dropped here: a client's own reaches the upstream under its
//! own name, and a provider allow-list cannot strip it. `x-grok-conv-id` is
//! channel-owned, because it mirrors the body the channel shaped.
//!
//! Not ported from v3: `CountTokens`, which v3 answered locally and v4's host
//! owns; `EditVideo` and `ExtendVideo`, which `gproxy-protocol` has no
//! operation for; the `ResourceMutation`/`settlement_ready` video bookkeeping,
//! which has no v4 counterpart; the media request and response rewrites,
//! dropped for the same reason `xai` dropped them (a multipart body is
//! forwarded as the caller wrote it and a job reply is xAI's own shape); the
//! `endpoints` map, which v4 expresses as the host's per-operation
//! `endpoint_override`; and the `ChannelTrafficPolicy`, which has no v4
//! counterpart — a provider that wants v3's "forward nothing" writes
//! `allowed_headers: []`.

mod auth;
mod config;
mod oauth;
mod quota;
mod shape;
mod usage;

pub use auth::{CLI_USER_AGENT, CLI_VERSION};
pub use config::{
    DEFAULT_BASE_URL, DEFAULT_CLIENT_ID, DEFAULT_MEDIA_BASE_URL, DEFAULT_TOKEN_URL,
    GrokBuildConfig, ID, OAUTH_SCOPE,
};
pub use quota::{TOP_UP_URL, USAGE_DIMENSION};
pub use usage::{COST_TICKS_METRIC, UPSTREAM_COST_METRIC, UPSTREAM_PRICED_DIMENSION};

use crate::channel::{UsageExtras, 
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthDeviceCode, PrepareContext, ProviderView, QuotaQuery, forwardable,
};
use crate::channels::shared::compatible::http::strip_query_auth;
use config::base_url;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::Method;
use serde_json::{Map, Value};

#[derive(Debug, Default, Clone, Copy)]
pub struct GrokBuild;

/// What the CLI itself sends and what the channel must not let an allow-list
/// hide: its own identity block, and the session id the gateway's ladder
/// reads out of the client's request.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        auth::SESSION_HEADER,
        "x-grok-conv-id",
        "x-grok-user-id",
        "x-grok-client-version",
        "x-grok-client-identifier",
        "x-grok-client-mode",
        "x-xai-token-auth",
        "x-authenticateresponse",
        "user-agent",
    ],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. `x-grok-session-id`
/// is deliberately absent: it is the client's session, not the channel's.
const CHANNEL_HEADERS: &[&str] = &[
    "x-xai-token-auth",
    "x-authenticateresponse",
    "x-grok-client-version",
    "x-grok-client-identifier",
    "x-grok-client-mode",
    "x-grok-conv-id",
    "x-grok-user-id",
    "x-userid",
    "user-agent",
    "accept",
    "cookie",
];

pub(super) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

pub(super) fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", encode_component(name), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Which origin an operation belongs to. Generation and the catalogue are the
/// OAuth account's; the media paths are xAI's public ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    ChatProxy,
    Media,
}

fn surface(operation: Operation, dialect: Dialect) -> Surface {
    let media = dialect == Dialect::OpenAi
        && matches!(
            operation,
            Operation::CompactContent
                | Operation::CreateImage
                | Operation::EditImage
                | Operation::CreateSpeech
                | Operation::CreateTranscription
                | Operation::CreateVideo
                | Operation::RetrieveVideo
        );
    if media {
        Surface::Media
    } else {
        Surface::ChatProxy
    }
}

/// The path under the surface's origin. Both origins already carry `/v1`, so
/// the caller's own `/v1` prefix comes off; xAI's three renamed paths are
/// substituted (v3 `prepare.rs::path`, `xai/request.rs::path`).
fn operation_path(operation: Operation, surface: Surface, path: &str) -> String {
    if surface == Surface::Media {
        match operation {
            Operation::CreateSpeech => return "/tts".into(),
            Operation::CreateTranscription => return "/stt".into(),
            Operation::CreateVideo => return "/videos/generations".into(),
            _ => {}
        }
    }
    let path = path.strip_prefix("/v1").unwrap_or(path);
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

/// xAI's speech API is not OpenAI's: the text is `text`, the voice is
/// `voice_id` and the container is `output_format.codec`; `model`,
/// `instructions` and `stream_format` have no counterpart (v3
/// `grokbuild/shape/media.rs`).
fn speech(body: &[u8]) -> Result<gproxy_protocol::connection::Bytes, ChannelError> {
    let mut object: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|_| shape::invalid("a speech request must be a JSON object"))?;
    for name in ["model", "instructions", "stream_format"] {
        object.remove(name);
    }
    if let Some(input) = object.remove("input") {
        object.entry("text").or_insert(input);
    }
    if let Some(voice) = object.remove("voice") {
        object.entry("voice_id").or_insert(voice);
    }
    if let Some(format) = object.remove("response_format") {
        object
            .entry("output_format")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(output) = object
            .get_mut("output_format")
            .and_then(Value::as_object_mut)
        {
            output.insert("codec".into(), format);
        }
    }
    Ok(gproxy_protocol::connection::Bytes::from(
        Value::Object(object).to_string(),
    ))
}

impl BaseChannel for GrokBuild {
    fn id(&self) -> &'static str {
        ID
    }

    /// An xAI account through the Grok Build CLI: a device login, a
    /// refreshable token, and the credit window the chat proxy reports.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Grok Build (xAI account)",
            login_modes: vec![LoginMode::DeviceCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Chat proxy origin; defaults to https://cli-chat-proxy.grok.com/v1. Provider column, not config JSON.",
                ).with_placeholder(config::DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "media_base_url",
                    ConfigKeyKind::String,
                    "Origin for xAI's own image, speech, transcription and video paths; defaults to https://api.x.ai/v1.",
                ).with_placeholder(config::DEFAULT_MEDIA_BASE_URL),
                ConfigKey::optional(
                    "usage_base_url",
                    ConfigKeyKind::String,
                    "Origin the billing probe reads; defaults to the chat proxy, which is the only surface that answers it.",
                ).with_placeholder("{base_url}"),
                ConfigKey::optional(
                    "oauth_device_code_url",
                    ConfigKeyKind::String,
                    "Device authorization endpoint; defaults to https://auth.x.ai/oauth2/device/code.",
                ).with_placeholder(config::DEFAULT_DEVICE_CODE_URL),
                ConfigKey::optional(
                    "oauth_token_url",
                    ConfigKeyKind::String,
                    "Token endpoint used by the device poll and by refresh; defaults to https://auth.x.ai/oauth2/token.",
                ).with_placeholder(config::DEFAULT_TOKEN_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// v3 sent no connection profile for Grok Build and there is no capture of
    /// the CLI's ClientHello to reproduce, so the operator's own profile is
    /// the only thing that decides the outbound stack.
    fn default_connection(&self) -> Option<gproxy_client::ConnectionConfig> {
        None
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAi, Dialect::OpenAiChat]
            }
            Operation::ListModels
            | Operation::GetModel
            | Operation::CompactContent
            | Operation::CreateImage
            | Operation::EditImage
            | Operation::CreateSpeech
            | Operation::CreateTranscription
            | Operation::CreateVideo
            | Operation::RetrieveVideo => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = GrokBuildConfig::from_view(ctx.provider)?;
        let OperationKey { operation, dialect } = ctx.operation;
        if self.native_dialects(ctx.provider, operation).is_empty() {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let surface = surface(operation, dialect);
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            method,
            path,
            query,
            headers: source,
            body,
        } = ctx.request;

        let generation = matches!(
            operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        );
        let (body, conversation) = match body {
            HttpBody::Bytes(bytes) if generation && dialect == Dialect::OpenAi => {
                let shaped = shape::request(&bytes)?;
                let conversation = shape::cache_key(&shaped);
                (HttpBody::Bytes(shaped), conversation)
            }
            HttpBody::Bytes(bytes) if operation == Operation::CreateSpeech => {
                (HttpBody::Bytes(speech(&bytes)?), None)
            }
            // A Chat body and every media body reach the upstream as the
            // caller wrote them; the conversation id is still mirrored when
            // the caller stated one.
            HttpBody::Bytes(bytes) => {
                let conversation = shape::cache_key(&bytes);
                (HttpBody::Bytes(bytes), conversation)
            }
            other => (other, None),
        };

        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => {
                let base = match surface {
                    Surface::ChatProxy => base_url(ctx.provider),
                    Surface::Media => config.media_base_url().to_owned(),
                };
                format!("{base}{}", operation_path(operation, surface, &path))
            }
        };
        let uri = match query
            .map(|query| strip_query_auth(&query))
            .filter(|query| !query.is_empty())
        {
            Some(query) if url.contains('?') => format!("{url}&{query}"),
            Some(query) => format!("{url}?{query}"),
            None => url,
        };

        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        let reply = match operation {
            Operation::CreateSpeech => auth::Reply::Audio,
            Operation::StreamGenerateContent => auth::Reply::Stream,
            _ => auth::Reply::Json,
        };
        auth::apply(
            &mut headers,
            &config,
            &ctx.credential,
            reply,
            conversation.as_deref(),
        )?;
        let mut builder = http::Request::builder()
            .method(if method == Method::default() {
                Method::POST
            } else {
                method
            })
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn usage_extras(&self) -> Option<&dyn UsageExtras> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn media_leaves_the_chat_proxy_and_generation_stays_on_it() {
        assert_eq!(
            surface(Operation::CreateSpeech, Dialect::OpenAi),
            Surface::Media
        );
        assert_eq!(
            surface(Operation::StreamGenerateContent, Dialect::OpenAi),
            Surface::ChatProxy
        );
        assert_eq!(
            surface(Operation::ListModels, Dialect::OpenAi),
            Surface::ChatProxy
        );
    }

    #[test]
    fn a_caller_path_loses_the_v1_both_origins_already_carry() {
        assert_eq!(
            operation_path(Operation::ListModels, Surface::ChatProxy, "/v1/models"),
            "/models"
        );
        assert_eq!(
            operation_path(Operation::GetModel, Surface::ChatProxy, "/v1/models/grok-4"),
            "/models/grok-4"
        );
        assert_eq!(
            operation_path(Operation::CreateSpeech, Surface::Media, "/v1/audio/speech"),
            "/tts"
        );
        assert_eq!(
            operation_path(Operation::CreateVideo, Surface::Media, "/v1/videos"),
            "/videos/generations"
        );
    }

    #[test]
    fn a_speech_request_is_renamed_into_xais_own_fields() {
        let body = json!({"model": "grok-voice", "input": "hello", "voice": "ara",
                          "response_format": "mp3", "stream_format": "sse", "speed": 1.2})
        .to_string();
        let out: Value = serde_json::from_slice(&speech(body.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            out,
            json!({"text": "hello", "voice_id": "ara",
                   "output_format": {"codec": "mp3"}, "speed": 1.2})
        );
    }
}
