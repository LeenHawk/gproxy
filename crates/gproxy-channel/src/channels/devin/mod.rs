//! Devin (Windsurf) through the hosted `server.codeium.com` backend.
//!
//! The only v4 channel whose upstream is not HTTP with JSON or SSE: it speaks
//! **Connect-RPC over protobuf**. A request is one envelope, the response a
//! multi-frame stream, and this channel translates those frames into OpenAI
//! Chat Completions for the client.
//!
//! # Where the facts come from
//!
//! The original wire implementation follows two local reference clients.
//! OAuth login and native tool calls were subsequently verified against the
//! live service with CLI clients; the unverified fields remain noted below.
//!
//! * `samples/windsurfapi` (dwgx/WindsurfAPI) — a working adapter for this
//!   exact endpoint, and the only mirror that carries live captures.
//!   `src/devin-connect.js` holds the calibration record in its header, the
//!   request/response tag maps in its builders and the error taxonomy in
//!   `classifyUpstreamError`; `src/connect.js` the envelope framing;
//!   `src/data/devin-catalog-snapshot.json` the captured selector catalogue,
//!   copied into this crate's `assets/`; `src/stop-sequences.js` the local
//!   stop-sequence enforcement; `docs/DEVIN-CONNECT-CUTOVER.md` the
//!   operational findings, including which tags are measured and which are
//!   only declaration order in someone's `.proto`.
//! * `samples/cpa-manager-plus` (seakee/CPA-Manager-Plus) — **not** a client
//!   for this upstream. Its own release notes say "Devin request execution and
//!   authentication remain CPA-owned. CPAMP only manages, presents, and calls
//!   the existing Management API", so it is evidence about exactly two things:
//!   the quota payload's shape (`utils/quota/devinQuota.ts`,
//!   `utils/quota/constants.ts`) and a second, independent statement of the
//!   `GetUserStatus` JSON endpoint. Anything it calls "Devin OAuth" is
//!   CLIProxyAPI's own management API and says nothing about this upstream.
//!
//! Facts marked *inferred* below are this channel's own reasoning, not
//! something either mirror observed.
//!
//! Browser PKCE login additionally follows `samples/CLIProxyAPI` at
//! `e2bff010` (`internal/auth/devin/devin_auth.go`): authorization at
//! `app.devin.ai/auth/cli/continue`, JSON code exchange at
//! `api.devin.ai/auth/cli/token`, and the `devin-session-token$` JWT prefix.
//! The host owns PKCE state, verifier storage and credential persistence.
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
//! This channel stretches one from a seed that is **not** the session token:
//! the reference derives its stable-device value from a separate seed on
//! purpose, because the `info` label namespaces distinct fingerprints from one
//! seed *"so the #31 metadata and, later, the login UA cannot be
//! cross-correlated or reversed"*. The seed here is the credential's host
//! identity (provider and credential id), so the device is stable for the
//! credential's lifetime, nothing has to be persisted, the value is not a
//! function of the secret, and two installations that paste the same token do
//! not present the same device. A credential may name its own `device_seed`
//! to keep a device across a re-add, or carry a literal 732-character
//! `fingerprint`; a value of the wrong shape is refused here rather than
//! upstream. The length requirement is the upstream's, the construction is
//! *inferred*.
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
//! an optional short-lived user JWT, #31 the fingerprint. The duplicated pairs
//! are not echoes: the JSON codec's body for the same message is
//! `{ideName, ideVersion, apiKey, locale, os, extensionName, extensionVersion,
//! clientName}`, which maps positionally onto #1 `ide_name`, #2 `ide_version`,
//! #3 `api_key`, #4 `locale`, #5 `os`, #7 `extension_version` and #12
//! `client_name`. That map has **no slot for `extensionName`**, which is why
//! the protobuf request omits a field the JSON quota body sends: inventing a
//! field number is the one thing this channel does not do. Note also that #3
//! is `api_key` and not the `session_token` the reference calls it.
//!
//! `ChatMessage`: #1 uuid, #2 source (`1` user, `2` assistant, `4` tool
//! result), #3 text, #6 a native tool call, #7 the tool-call id a tool result
//! answers, #10 repeated images as `{1: base64 text, 2: mime type}`. This
//! channel emits these fields, preserving native tool calls and results.
//!
//! Two rules shape the turn list before it is encoded, both from the
//! reference and both narrower than they look:
//!
//! * **An assistant turn with no text or tool calls is dropped**, images or not. The Kimi
//!   workaround records 10/10 empty retries for an empty assistant turn, and
//!   `devin-connect-openai.js` adds that empty assistant turns poison the
//!   upstream into repeating empty turns. Image-only assistant turns are
//!   covered by the same exclusion in the reference ("Image-only assistants
//!   retain the legacy exclusion"): admitting one puts back exactly the
//!   text-empty frame the exclusion exists to prevent.
//! * **A same-source run is merged only once it would reach three.** The
//!   upstream validator rejects a run of three or more with
//!   `invalid_argument`; a run of two is tolerated. Merging every run would
//!   be safe and is what the reference does, but it rewrites the conversation
//!   the model is shown more than the wire requires, so this channel merges
//!   only the turn that would cross the threshold. A turn carrying images
//!   never merges — it encodes to a different shape.
//!
//! `CompletionConfig`: #1 enabled, #2 `max_tokens`, #3 the context window, #5
//! temperature (double), #7 `top_k`, #8 `top_p` (double). The #2/#3 pair was
//! corrected in the reference after a capture showed the output cap sitting on
//! `max_newlines`; `DEVIN-CONNECT-CUTOVER.md` §8.9 records both the fix and
//! that enforcement of #2 is a schema-backed expectation rather than a
//! measured fact. #3 is the contradiction the reference never resolved: it
//! *declares* the field `max_newlines` and *writes* the context window into
//! it, reconciling the two with "a large max_newlines is a no-op". This
//! channel follows the use rather than the declaration and calls the key
//! `context_window`, because an operator who reaches for it is trying to move
//! a context limit. An exact temperature of `0` reliably answers "an internal
//! error occurred" while `0.001` succeeds, so a request for greedy sampling is
//! clamped to that floor (live finding, same section).
//!
//! Request fields this channel deliberately does not write: #11
//! `disable_parallel_tool_calls`, #12 `tool_choice`, #13
//! prompt-cache options — all recorded in the mirrors as declaration order
//! from third-party `.proto` files rather than as wire captures — and #22
//! `request_id`, which the verified turn-1 request omits.
//!
//! # `GetChatMessageResponse`
//!
//! #3 answer text, #5 stop enum, #6 native tool-call deltas, #7 metadata,
//! #9 thinking text. **The
//! reference file header says the deltas ride #9; its own later calibration
//! corrects that** ("Earlier code read #9 as the content — that was the
//! thinking stream. The answer the caller actually wants is #3"), and this
//! channel follows the correction. Inside #7: #2 fresh prompt tokens, #3
//! completion tokens, #4 cache-creation tokens, #5 cache-read tokens, #9 the
//! model that actually served the turn. The cache tags were calibrated on a
//! paid account by replaying one prefix (`DEVIN-CONNECT-CUTOVER.md` §8.0.1);
//! on a free account they are absent because they are zero and protobuf omits
//! zero scalars. #9 is reported as a usage dimension (`actual_model`) and not
//! as the response `model`: clients compare `model` against what they asked
//! for, but #9 is the only signal of what a router or a family alias actually
//! ran, and therefore of what is being billed.
//!
//! Of the stop enum only `2` (free completion) and `4` (paid completion) are
//! pinned, and both mean a clean stop. Native calls finish as `tool_calls`;
//! truncation is inferred from the answer landing exactly on the caller's cap
//! (§8.7, which also records the retired guesses that made complete answers
//! read as truncated).
//!
//! The caller's `stop` sequences have no field on this wire at all. The
//! reference enforces them locally rather than dropping them, and so does
//! `stream.rs`: the answer is truncated at the first occurrence, the matched
//! text is not returned, and the turn finishes as `stop`.
//!
//! # Errors
//!
//! **This upstream wraps transient faults in a 401/403 auth shell.** A
//! capacity throttle, a backend "an internal error occurred (trace ID: …)" and
//! a content-policy block all arrive as `permission_denied`, so the status is
//! actively misleading and reading it at face value retires live credentials.
//! `error.rs` ports the reference's classifier, match order included, and maps
//! each class onto the status that describes it: 400 for a permanent client
//! mistake and for a content-policy block, 401 for a real auth failure, 403
//! for a tier wall, 429 for a throttle or an exhausted account, 502 for a
//! transient backend fault and 503 for capacity. A classified refusal is
//! handed back as an *answer* rather than as a channel failure, because
//! `gproxy-core::execute::attempt::classify` reads the status: the two 400s
//! are `Final`, so the caller sees them and the credential is neither blamed
//! nor retried, while 429 and 5xx exclude this credential and move on. The one
//! case that cannot be answered is a failure that arrives after content has
//! already been streamed; there the status is spent and the stream fails.
//!
//! # Models
//!
//! `model_selector` takes strings such as `swe-1-6-slow` or
//! `MODEL_SWE_1_5_SLOW`. The catalogue is the 123-selector capture in
//! `assets/devin-catalog-snapshot.json`, copied from the reference's own
//! `GetCliModelConfigs`/`GetCascadeModelConfigs` capture and dated
//! `2026-07-08`, plus the hand-written alias table in `models.rs` that gives
//! client-facing names a specific target. **The snapshot is dated, and a live
//! `GetCliModelConfigs` is the authority**; this channel does not call it
//! because the upstream's list is an account-entitlement view rather than the
//! set of strings #21 accepts, and `config.models` is the escape valve for
//! anything added since. A free-tier account resolves only `swe-1-6-slow`;
//! every other selector answers with an upgrade message. That is an
//! account-tier wall, not a protocol gap, so the channel sends the selector
//! and reports the refusal rather than pre-filtering. A name the catalogue
//! does not know is **refused**: the reference removed its silent degrade to
//! the free selector once it was clear that answering a paid request on the
//! free model changes both the answer and the billing
//! (`DEVIN-CONNECT-CUTOVER.md` §1).
//!
//! # Quota
//!
//! `GetUserStatus` over Connect's **JSON** codec, with the token in the body.
//! See `quota.rs`. Whether the daily and weekly windows are rolling or
//! calendar-aligned is *inferred*: only a reset instant is reported, and they
//! are declared rolling because that is what the other percentage-window
//! channels do. The payload's billing ledger — overage balance, prompt and
//! flex credits, plan period — is reported alongside them as balance and
//! budget entries with no dimension, so it is recorded and never blocks.
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
//! * **Forced tool choice.** Native function declarations, history, streamed
//!   calls and tool results are supported; `tool_choice` supports `auto` and
//!   `none`. Other choices are refused rather than silently weakened.
//! * **Router selectors** (`adaptive`, `arena-*`). They need an `AssignModel`
//!   round trip to resolve to a concrete `model_uid` first, and every field
//!   number of that method is a guess in both mirrors — the reference labels
//!   them "sequential from 1, the prost default". Sending a router name
//!   straight to #21 fails upstream with an opaque internal error, so
//!   `models.rs` refuses it with a message that says which of the two problems
//!   it is.
//! * **Credential refresh.** The PKCE exchange returns a session token but no
//!   refresh token or documented refresh endpoint. Reauthorize when it expires.
//! * **The short-lived user JWT** (`/exa.auth_pb.AuthService/GetUserJwt`,
//!   HS256, roughly 24 minutes, carried as `ClientMetadata` #21). The
//!   reference ships it default-off because the no-JWT wire is known to work.
//!   This channel sends it when the credential carries one and never mints it,
//!   which would add a second round trip per call for a field the upstream
//!   does not require.
//! * **The `CheckUserMessageRateLimit` pre-flight**, a free capacity probe
//!   returning `retryAfterMs`. A pre-flight on every request doubles the round
//!   trips for a signal the error taxonomy already recovers after the fact.
//! * **A `default_connection`.** The reference dials `server.codeium.com` with
//!   Node's stock TLS and succeeds, so `connect-es` over plain HTTPS has no
//!   fingerprint to reproduce and the channel names no client profile
//!   (*inferred*: neither mirror tested a fingerprinted client).
//!
//! # Open questions a live account would settle
//!
//! Nothing below can be closed by reading the mirrors, and they are ordered by
//! what each would unlock.
//!
//! 1. **Whether `GetChatMessage` accepts the Connect JSON codec.** This is the
//!    highest-value single test and nobody has run it. Five sibling methods on
//!    this same origin take `content-type: application/json` with the token in
//!    the body and no authorization header. If the chat method does too, then
//!    most of the field-number archaeology above becomes unnecessary: the JSON
//!    body is self-describing, field *names* replace reverse-engineered
//!    *numbers*, and the uncalibrated #11/#12/#13/#22/#26/#27 stop being
//!    guesses. It costs one session token and one request.
//! 2. Native tool calls now use #6 with #1 id, #2 name and #3 JSON argument
//!    fragments, following CLIProxyAPI and verified by CLI execution. Native
//!    thinking-signature replay remains outside this channel's Chat surface.
//! 3. **A truncated or refused turn, captured.** The only way to pin stop-enum
//!    integers for `length`, `content_filter` and `tool_calls`, which in turn
//!    is the only way to stop inferring truncation from the caller's cap.
//!    Requires `max_tokens` to actually be enforced, which is itself unproven.
//! 4. **An `AssignModel` round trip on a router selector.** One capture pins
//!    all five of its guessed field numbers and unblocks `adaptive`/`arena-*`.
//! 5. **A paid capture of the top-level billing fields** #14/#22/#26/#27.
//!    Structurally impossible on a free account: they are billed zero and
//!    protobuf omits zero scalars.
//!
//! Installing the Windsurf client is the **lowest**-value item on this list,
//! not the highest. A live binary capture would only explain the meaning of
//! request #7 = 5 and #20 = 1, whether #31 has semantics beyond its length,
//! the `tool_choice` option-name strings, and which client version is current
//! — and the three versions the mirrors disagree about all work. Items 1 to 5
//! need a token, not a binary.

mod config;
pub mod connect;
pub mod error;
mod models;
mod oauth;
mod prompt;
pub mod proto;
mod quota;
mod reason;
mod request;
mod stream;
mod usage;

pub use config::{CLIENT_NAME, CLIENT_VERSION, DevinConfig, ID};
pub use error::{ErrorClass, classify};
pub use models::{CALIBRATED_AT, FREE_SELECTOR, ROUTER_SELECTORS, SELECTORS, catalogue, resolve};
pub use quota::{DAILY_ID, FLEX_CREDITS_ID, OVERAGE_ID, PROMPT_CREDITS_ID, WEEKLY_ID};
pub use request::{FINGERPRINT_HEX_CHARS, derive_fingerprint, validate_fingerprint};
pub use stream::{ACTUAL_MODEL_KEY, Usage};
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
    QuotaModel, QuotaQuery, UsageExtras, forwardable,
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
/// free-tier upgrade wall arrives that way — so it is read whole and put
/// through the taxonomy in `error.rs` before it is reported.
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
        let body = read_body(response.body, MAX_SERVICE_BODY).await?;
        return Err(error::from_response(response.status, &body));
    }
    let codec = stream::Codec::new(
        format!("chatcmpl-{}", request::uuid()?.replace('-', "")),
        prepared.model,
        now_secs(),
        prepared.max_tokens,
        prepared.stop,
    );
    Ok(stream::Turn::new(body_stream(response.body), codec))
}

/// Deliver a classified refusal as the answer instead of as a channel
/// failure. `gproxy-core::execute::attempt::classify` reads the status of an
/// answer and nothing of the status inside an error, so this is what makes the
/// taxonomy count: a 400 content block is `Final` (returned to the caller,
/// credential untouched, never retried), while a 429 or a 5xx excludes this
/// credential and lets the next one try. `CapabilityError` says the same thing
/// from the other side — a non-2xx upstream reply is still a successful send.
/// Anything that is not a classified refusal stays an error.
fn deliver(error: ChannelError) -> Result<WireResponse<HttpBody>, ChannelError> {
    match error {
        ChannelError::UpstreamResponse { status, body } => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Bytes(body),
            })
        }
        other => Err(other),
    }
}

impl BaseChannel for Devin {
    fn id(&self) -> &'static str {
        ID
    }

    /// Browser PKCE login; no token refresh endpoint.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Devin (Windsurf, server.codeium.com)",
            login_modes: vec![LoginMode::AuthorizationCode],
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
                ).with_placeholder(connect::DEFAULT_BASE_URL),
                ConfigKey::optional("authorize_url", ConfigKeyKind::String, "Devin browser PKCE authorization endpoint.")
                    .with_placeholder(config::DEFAULT_AUTHORIZE_URL),
                ConfigKey::optional("token_url", ConfigKeyKind::String, "Devin authorization-code exchange endpoint.")
                    .with_placeholder(config::DEFAULT_TOKEN_URL),
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
                    "Extra client-facing model name to upstream selector entries, merged over the built-in catalogue. The built-in catalogue is a dated capture, so this is where a selector the upstream added since goes. An unknown name is refused, never downgraded to the free selector.",
                ),
                ConfigKey::optional(
                    "max_tokens",
                    ConfigKeyKind::Integer,
                    "CompletionConfig #2 when the request names no cap.",
                ),
                ConfigKey::optional(
                    "context_window",
                    ConfigKeyKind::Integer,
                    "CompletionConfig #3. The reference declares this field max_newlines and writes the context window into it; the key is named for the use, not the declaration. Also accepted as max_newlines.",
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
            let turn = match start(context).await {
                Ok(turn) => turn,
                Err(error) => return deliver(error),
            };
            let completion = match turn.collect().await {
                Ok(completion) => completion,
                Err(error) => return deliver(error),
            };
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
            let mut turn = match start(context).await {
                Ok(turn) => turn,
                Err(error) => return deliver(error),
            };
            // The first batch is pulled before the headers are committed: an
            // upstream refusal usually *is* the first thing on the stream, and
            // until a byte is written it can still be answered with a status.
            let primed = match turn.prime().await {
                Ok(chunks) => chunks,
                Err(error) => return deliver(error),
            };
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Stream(turn.into_stream(primed)),
            })
        })
    }

    fn oauth_authorization_code(&self) -> Option<&dyn crate::channel::OAuthAuthorizationCode> {
        Some(self)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }

    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }

    fn response_reason_observer(
        &self,
        status: http::StatusCode,
        headers: &http::HeaderMap,
        max_bytes: u64,
    ) -> Option<Box<dyn crate::channel::ResponseReasonObserver>> {
        reason::observer(status, headers, max_bytes)
    }

    fn usage_extras(&self) -> Option<&dyn UsageExtras> {
        Some(self)
    }
}
