//! Devin (Windsurf) through the hosted `server.codeium.com` backend.
//!
//! The only v4 channel whose upstream is not HTTP with JSON or SSE: it speaks
//! **Connect-RPC over protobuf**. A request is one envelope, the response a
//! multi-frame stream, and this channel translates those frames into OpenAI
//! Chat Completions for the client.
//!
//! # Where the facts come from
//!
//! Nothing here was verified against the live service. Two local mirrors of
//! other people's reverse-engineering are the evidence, and each fact below
//! names which one:
//!
//! * `samples/windsurfapi` (dwgx/WindsurfAPI) — a working adapter for this
//!   exact endpoint. `src/devin-connect.js` carries the calibration record in
//!   its header and the request/response tag maps in its builders;
//!   `src/connect.js` the envelope framing; `src/devin-connect-models.js` the
//!   captured selector catalogue; `docs/DEVIN-CONNECT-CUTOVER.md` the
//!   operational findings, including which tags are measured and which are
//!   only declaration order in someone's `.proto`.
//! * `samples/cpa-manager-plus` (seakee/CPA-Manager-Plus) — a management
//!   panel whose Devin provider reads the quota payload
//!   (`utils/quota/devinQuota.ts`, `utils/quota/constants.ts`).
//!
//! Facts marked *inferred* below are this channel's own reasoning, not
//! something either mirror observed.
//!
//! # Transport
//!
//! `POST {base}/exa.api_server_pb.ApiServerService/GetChatMessage` with
//! `content-type: application/connect+proto`, `connect-protocol-version: 1`,
//! `connect-accept-encoding: gzip` and `user-agent: connect-es/2.0.0`
//! (`devin-connect.js` header; `connect.js::connectHeaders`). An envelope is
//! `[flags][big-endian length][payload]`, flag `0x01` gzip and `0x02`
//! end-of-stream, whose payload is a JSON trailer: `{}` for success,
//! `{"error":{…}}` for failure. The request frame goes out **uncompressed** —
//! a gzipped request frame is rejected with an opaque internal error, while
//! the server still gzips frames on the way back (`devin-connect.js`, the
//! comment above its `wrapEnvelope(proto, { compress: false })`). That is why
//! `connect.rs` inflates but never deflates.
//!
//! # Authentication
//!
//! `authorization: Basic <token>-<token>` — the session token **doubled and
//! dash-joined**. A single token is refused with `permission_denied`. The
//! copy inside the message (`ClientMetadata.session_token`, #3) stays
//! **single** (`devin-connect.js` header §1 and `buildClientMetadata`).
//!
//! `ClientMetadata` #31 is a device fingerprint of **732 hexadecimal
//! characters (366 bytes)**; a shorter value trips a server-side "internal"
//! error. The server checks the shape and not the value, so any hex string of
//! that length works (`devin-connect.js` header §2, `generateFingerprint`).
//! This channel derives one deterministically from the session token so a
//! credential keeps the same device identity without anything to persist; the
//! length requirement is theirs, the derivation is *inferred* — the reference
//! derives its own stable-device value from an HMAC over a separate seed, and
//! nothing observed says the upstream cares either way. A credential may carry
//! its own `fingerprint` instead, and a value of the wrong shape is refused
//! here rather than upstream.
//!
//! # `GetChatMessageRequest`
//!
//! | # | Field | Note |
//! |---|---|---|
//! | 1 | `ClientMetadata` | see below |
//! | 2 | `system_prompt` | present even when empty |
//! | 3 | `ChatMessage` | repeated, in order |
//! | 7 | constant `5` | meaning not recorded |
//! | 8 | `CompletionConfig` | see below |
//! | 15 | `ModelConfig` | `{1: session config uuid, 2: turn counter, 3: 4}` |
//! | 16 | `session_id` | uuid |
//! | 20 | constant `1` | meaning not recorded |
//! | 21 | `model_selector` | a **string** selector, not an enum ordinal |
//!
//! `ClientMetadata`: #1 and #12 the client name (`chisel`), #2 and #7 the CLI
//! version, #3 the session token, #4 the locale, #5 the operating system, #21
//! an optional short-lived user JWT, #31 the fingerprint.
//!
//! `ChatMessage`: #1 uuid, #2 source (`1` user, `2` assistant, `4` tool
//! result), #3 text, #6 a native tool call, #7 the tool-call id a tool result
//! answers, #10 repeated images as `{1: base64 text, 2: mime type}`. This
//! channel emits #1, #2, #3 and #10; tool turns are folded into user text
//! because emitting #6 requires the `ToolDef` tags of request field #10, which
//! the mirrors record as uncalibrated and ship default-off.
//!
//! `CompletionConfig`: #1 enabled, #2 `max_tokens`, #3 `max_newlines`, #5
//! temperature (double), #7 `top_k`, #8 `top_p` (double). The #2/#3 pair was
//! corrected in the reference after a capture showed the output cap sitting on
//! `max_newlines`; `DEVIN-CONNECT-CUTOVER.md` §8.9 records both the fix and
//! that enforcement of #2 is a schema-backed expectation rather than a
//! measured fact. An exact temperature of `0` reliably answers "an internal
//! error occurred" while `0.001` succeeds, so a request for greedy sampling is
//! clamped to that floor (live finding, same section).
//!
//! Request fields this channel deliberately does not write: #10 native tool
//! definitions, #11 `disable_parallel_tool_calls`, #12 `tool_choice`, #13
//! prompt-cache options — all recorded in the mirrors as declaration order
//! from third-party `.proto` files rather than as wire captures — and #22
//! `request_id`, which the verified turn-1 request omits.
//!
//! # `GetChatMessageResponse`
//!
//! #3 answer text, #5 stop enum, #7 metadata, #9 thinking text. **The
//! reference file header says the deltas ride #9; its own later calibration
//! corrects that** ("Earlier code read #9 as the content — that was the
//! thinking stream. The answer the caller actually wants is #3"), and this
//! channel follows the correction. Inside #7: #2 fresh prompt tokens, #3
//! completion tokens, #4 cache-creation tokens, #5 cache-read tokens, #9 the
//! model that actually served the turn. The cache tags were calibrated on a
//! paid account by replaying one prefix (`DEVIN-CONNECT-CUTOVER.md` §8.0.1);
//! on a free account they are absent because they are zero and protobuf omits
//! zero scalars. #9 is not read: nothing in the v4 contract consumes it, and
//! it must not be echoed as the response `model`, which clients compare
//! against what they asked for.
//!
//! Of the stop enum only `2` (free completion) and `4` (paid completion) are
//! pinned, and both mean a clean stop, so every value maps to `stop` and
//! truncation is inferred from the answer landing exactly on the caller's cap
//! (§8.7, which also records the retired guesses that made complete answers
//! read as truncated).
//!
//! # Models
//!
//! `model_selector` takes strings such as `swe-1-6-slow` or
//! `MODEL_SWE_1_5_SLOW`, captured from a live `GetCliModelConfigs` response
//! and ported into `models.rs`. A free-tier account resolves only
//! `swe-1-6-slow`; every other selector answers with an upgrade message. That
//! is an account-tier wall, not a protocol gap, so the channel sends the
//! selector and reports the refusal rather than pre-filtering. A name the
//! catalogue does not know is **refused**: the reference removed its silent
//! degrade to the free selector once it was clear that answering a paid
//! request on the free model changes both the answer and the billing
//! (`DEVIN-CONNECT-CUTOVER.md` §1).
//!
//! # Quota
//!
//! `GetUserStatus` over Connect's **JSON** codec, with the token in the body.
//! See `quota.rs`. Whether the daily and weekly windows are rolling or
//! calendar-aligned is *inferred*: only a reset instant is reported, and they
//! are declared rolling because that is what the other percentage-window
//! channels do.
//!
//! # Why the protobuf is hand-written
//!
//! `gproxy-protocol` knows HTTP with JSON and SSE, and no crate in this
//! workspace's dependency graph carries `prost` or any other protobuf
//! implementation (checked with `cargo tree -p gproxy-channel --all-features`
//! and against `Cargo.lock`). Adding one — plus its build-time code
//! generation, plus a `.proto` we would have to write ourselves from
//! reverse-engineered field numbers, for a schema nobody publishes — would
//! cost the whole workspace a dependency for one channel. What this channel
//! actually needs is five field writers and a flat field reader, which is
//! `proto.rs`: about two hundred lines, schema-less by design, so an unknown
//! field the upstream adds parses and is ignored instead of failing a frame.
//! Teaching the shared wire layer a second encoding was rejected for the same
//! reason: one channel's transport is not the wire layer's business.
//!
//! The one dependency this does add is `miniz_oxide`, for the gzip frames the
//! server sends back. It is pure Rust, builds for wasm32, is already in the
//! lockfile, and is behind this channel's feature.
//!
//! # Not implemented, and why
//!
//! * **Login.** The panel mirror validates a Devin OAuth callback URL
//!   (`features/oauth/devinOAuth.ts`) but neither mirror records the
//!   authorization endpoint, the token exchange or the polling call, and the
//!   other mirror logs in with an email and password against a different
//!   origin. Inventing endpoints is worse than having none, so a credential is
//!   a pasted session token (`LoginMode::ApiKey`).
//! * **Credential refresh.** Same reason: nothing evidences a refresh
//!   endpoint for this session token.
//! * **The short-lived user JWT** (`/exa.auth_pb.AuthService/GetUserJwt`,
//!   HS256, roughly 24 minutes, carried as `ClientMetadata` #21). The
//!   reference ships it default-off because the no-JWT wire is known to work.
//!   This channel sends it when the credential carries one and never mints it,
//!   which would add a second round trip per call for a field the upstream
//!   does not require.
//! * **A `default_connection`.** The reference dials `server.codeium.com` with
//!   Node's stock TLS and succeeds, so `connect-es` over plain HTTPS has no
//!   fingerprint to reproduce and the channel names no client profile
//!   (*inferred*: neither mirror tested a fingerprinted client).

mod config;
pub mod connect;
mod models;
pub mod proto;
mod quota;
mod request;
mod stream;
mod usage;

pub use config::{CLIENT_NAME, CLIENT_VERSION, DevinConfig, ID};
pub use models::{FREE_SELECTOR, SELECTORS, catalogue, resolve};
pub use quota::{DAILY_ID, WEEKLY_ID};
pub use request::{FINGERPRINT_HEX_CHARS, derive_fingerprint, validate_fingerprint};
pub use stream::Usage;
pub use usage::CACHE_CREATION_METRIC;

use futures_util::StreamExt as _;
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    capability::{CapabilityError, CapabilityErrorKind, CapabilityErrorStage},
    connection::{ByteStream, Bytes},
};
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::OutboundClient;
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, OperationContext, OperationFuture, ProviderView,
    QuotaModel, QuotaQuery, UsageExtractor, UsageStream, forwardable,
};

/// Client requests may embed base64 images.
const MAX_REQUEST_BODY: usize = 64 * 1024 * 1024;
/// Account replies and upstream error bodies.
const MAX_SERVICE_BODY: usize = 16 * 1024 * 1024;

/// The channel. Stateless: one instance serves every provider that names it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Devin;

fn bad_request(message: impl Into<String>) -> ChannelError {
    ChannelError::Transport(CapabilityError::new(
        CapabilityErrorKind::Invalid,
        CapabilityErrorStage::Start,
        message,
    ))
}

/// Unix seconds for the `created` field of a completion.
fn now_secs() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

async fn read_body(body: HttpBody, limit: usize) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > limit {
                    return Err(ChannelError::InvalidResponse(
                        "body exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

async fn call(
    client: &dyn OutboundClient,
    request: http::Request<HttpBody>,
) -> Result<(StatusCode, Bytes), ChannelError> {
    let WireResponse { status, body, .. } = client.send(request).await?;
    Ok((status, read_body(body, MAX_SERVICE_BODY).await?))
}

fn body_stream(body: HttpBody) -> ByteStream {
    match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    }
}

fn local_response(content_type: &'static str, body: Bytes) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(body),
    }
}

fn unsupported(operation: Operation, dialect: Dialect) -> ChannelError {
    ChannelError::UnsupportedOperation(OperationKey { operation, dialect })
}

/// Encode the request, send the single envelope and hand back the translated
/// turn. A non-2xx reply is an error payload rather than Connect frames — the
/// free-tier upgrade wall arrives that way — so it is read and reported whole.
async fn start(context: OperationContext<'_>) -> Result<stream::Turn, ChannelError> {
    let config = DevinConfig::from_view(context.provider)?;
    let auth = request::auth(&context.credential)?;
    let url = match context.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!(
            "{}{}",
            DevinConfig::base_url(context.provider),
            connect::CHAT_PATH
        ),
    };
    let body = read_body(context.request.body, MAX_REQUEST_BODY).await?;
    let value = request::parse(&body)?;
    let prepared = request::build(&value, &config, &auth)?;

    let allowlist = HeaderAllowlist::from_view(context.provider)?;
    // The client's own headers may pass, narrowed by the provider, but every
    // header that identifies this transport is written afterwards so a client
    // cannot reshape the envelope or the session it is sent on.
    let mut headers = forwardable(
        &context.request.headers,
        allowlist.as_ref(),
        &[
            "connect-protocol-version",
            "connect-accept-encoding",
            "user-agent",
            "accept",
        ],
    );
    for (name, value) in connect::proto_headers(auth.token)? {
        if let Some(name) = name {
            headers.insert(name, value);
        }
    }
    config.static_headers(&mut headers)?;

    let mut builder = http::Request::post(&url);
    if let Some(slot) = builder.headers_mut() {
        *slot = headers;
    }
    let request = builder
        .body(HttpBody::Bytes(Bytes::from(connect::request_frame(
            &prepared.body,
        ))))
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    let response = context.client.send(request).await?;
    if !response.status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status: response.status,
            body: read_body(response.body, MAX_SERVICE_BODY).await?,
        });
    }
    let codec = stream::Codec::new(
        format!("chatcmpl-{}", request::uuid()?.replace('-', "")),
        prepared.model,
        now_secs(),
        prepared.max_tokens,
    );
    Ok(stream::Turn::new(body_stream(response.body), codec))
}

impl BaseChannel for Devin {
    fn id(&self) -> &'static str {
        ID
    }

    /// A pasted session token: no login flow in either mirror is complete
    /// enough to implement, and nothing evidences a refresh endpoint. The
    /// account's daily and weekly windows come from `GetUserStatus`.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Devin (Windsurf, server.codeium.com)",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                refresh: false,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Connect-RPC origin; defaults to https://server.codeium.com. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "client_name",
                    ConfigKeyKind::String,
                    "ClientMetadata #1 and #12; defaults to the CLI's own name.",
                ),
                ConfigKey::optional(
                    "client_version",
                    ConfigKeyKind::String,
                    "ClientMetadata #2 and #7; defaults to the captured CLI build.",
                ),
                ConfigKey::optional(
                    "locale",
                    ConfigKeyKind::String,
                    "ClientMetadata #4; defaults to en.",
                ),
                ConfigKey::optional(
                    "os",
                    ConfigKeyKind::String,
                    "ClientMetadata #5; defaults to windows, as the capture carried.",
                ),
                ConfigKey::optional(
                    "models",
                    ConfigKeyKind::Json,
                    "Extra client-facing model name to upstream selector entries, merged over the built-in catalogue. An unknown name is refused, never downgraded to the free selector.",
                ),
                ConfigKey::optional(
                    "max_tokens",
                    ConfigKeyKind::Integer,
                    "CompletionConfig #2 when the request names no cap.",
                ),
                ConfigKey::optional(
                    "max_newlines",
                    ConfigKeyKind::Integer,
                    "CompletionConfig #3; the reference writes the context window here.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every call.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// The client's dialect, not the upstream's: this channel converts
    /// protobuf frames into Chat Completions itself, so a host that routes a
    /// Chat Completions request here needs no conversion of its own.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels => vec![Dialect::OpenAiChat],
            _ => Vec::new(),
        }
    }

    /// Answered locally from the captured catalogue: the upstream's own model
    /// list is a separate Connect method, and its answer is an account
    /// entitlement view rather than the set of selectors #21 accepts.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::OpenAiChat {
                return Err(unsupported(Operation::ListModels, context.dialect));
            }
            let config = DevinConfig::from_view(context.provider)?;
            let body = serde_json::to_vec(&models::openai_list(&config.models))
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            Ok(local_response("application/json", Bytes::from(body)))
        })
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::OpenAiChat {
                return Err(unsupported(Operation::GenerateContent, context.dialect));
            }
            let completion = start(context).await?.collect().await?;
            let body = serde_json::to_vec(&completion)
                .map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
            Ok(local_response("application/json", Bytes::from(body)))
        })
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            if context.dialect != Dialect::OpenAiChat {
                return Err(unsupported(
                    Operation::StreamGenerateContent,
                    context.dialect,
                ));
            }
            let turn = start(context).await?;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Stream(turn.into_stream()),
            })
        })
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }

    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
