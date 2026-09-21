//! An OpenAI Chat Completions request rendered as `GetChatMessageRequest`.
//!
//! Field numbers are the ones the reference client puts on the wire; each is
//! named at its use site with the evidence class, and everything the
//! reference keeps behind a default-off calibration switch (native tool
//! definitions #10, tool_choice #12, prompt-cache options #13, request_id #22)
//! is deliberately not emitted here. See the module documentation in `mod.rs`.

use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::config::DevinConfig;
use super::models;
use super::proto::Message;
use crate::channel::{ChannelError, CredentialView};

// GetChatMessageRequest
const REQ_CLIENT_METADATA: u32 = 1;
const REQ_SYSTEM_PROMPT: u32 = 2;
const REQ_CHAT_MESSAGE: u32 = 3;
/// A constant 5 on every captured request; its meaning is not recorded.
const REQ_CONSTANT_SEVEN: u32 = 7;
const REQ_COMPLETION_CONFIG: u32 = 8;
const REQ_MODEL_CONFIG: u32 = 15;
const REQ_SESSION_ID: u32 = 16;
/// A constant 1 on every captured request; its meaning is not recorded.
const REQ_CONSTANT_TWENTY: u32 = 20;
const REQ_MODEL_SELECTOR: u32 = 21;

// ClientMetadata
const META_CLIENT_NAME: u32 = 1;
const META_CLIENT_VERSION: u32 = 2;
const META_SESSION_TOKEN: u32 = 3;
const META_LOCALE: u32 = 4;
const META_OS: u32 = 5;
const META_VERSION_ECHO: u32 = 7;
const META_CLIENT_NAME_ECHO: u32 = 12;
const META_USER_JWT: u32 = 21;
const META_FINGERPRINT: u32 = 31;

// ChatMessage
const MSG_UUID: u32 = 1;
const MSG_SOURCE: u32 = 2;
const MSG_TEXT: u32 = 3;
const MSG_IMAGES: u32 = 10;
const IMAGE_BASE64: u32 = 1;
const IMAGE_MIME: u32 = 2;

// CompletionConfig
const COMPLETION_ENABLED: u32 = 1;
const COMPLETION_MAX_TOKENS: u32 = 2;
/// Declared `max_newlines` by the reference and written the context window by
/// it; see `config::DEFAULT_CONTEXT_WINDOW` for the contradiction.
const COMPLETION_CONTEXT_WINDOW: u32 = 3;
const COMPLETION_TEMPERATURE: u32 = 5;
const COMPLETION_TOP_K: u32 = 7;
const COMPLETION_TOP_P: u32 = 8;

// ModelConfig
const MODEL_CONFIG_ID: u32 = 1;
const MODEL_CONFIG_TURN: u32 = 2;
const MODEL_CONFIG_CONSTANT: u32 = 3;

/// `ChatMessage.source`. Only the values this channel emits are listed.
const SOURCE_USER: u64 = 1;
const SOURCE_ASSISTANT: u64 = 2;

/// The smallest temperature the upstream accepts: an exact 0 reliably answers
/// "an internal error occurred" while 0.001 succeeds (live finding recorded in
/// `devin-connect.js::buildCompletionConfig`). OpenAI clients routinely ask
/// for 0, so the request is clamped rather than failed.
const MIN_TEMPERATURE: f64 = 0.001;
const DEFAULT_TEMPERATURE: f64 = 1.0;
const DEFAULT_TOP_K: u64 = 40;
const DEFAULT_TOP_P: f64 = 0.95;

/// `ClientMetadata` #31 must be 732 hexadecimal characters, i.e. 366 bytes.
/// A shorter value trips a server-side "internal" error; the value itself is
/// not session-bound, so any hex string of that length is accepted
/// (`devin-connect.js` file header §2 and `generateFingerprint`).
pub const FINGERPRINT_HEX_CHARS: usize = 732;

/// The session token and the client identity that travel with it.
pub(super) struct Auth<'a> {
    pub token: &'a str,
    pub fingerprint: String,
    pub user_jwt: Option<&'a str>,
}

/// Read the credential. The secret is `{"session_token": "..."}`, with
/// `api_key` accepted as the name the reference pool uses for the same value,
/// plus an optional `fingerprint`, an optional `device_seed` and an optional
/// short-lived `user_jwt`.
pub(super) fn auth<'a>(credential: &CredentialView<'a>) -> Result<Auth<'a>, ChannelError> {
    let token = ["session_token", "api_key", "token"]
        .into_iter()
        .find_map(|key| credential.secret.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let fingerprint = match credential.secret.get("fingerprint").and_then(Value::as_str) {
        Some(configured) => validate_fingerprint(configured.trim())?.to_owned(),
        None => derive_fingerprint(&fingerprint_seed(credential)),
    };
    Ok(Auth {
        token,
        fingerprint,
        user_jwt: credential
            .secret
            .get("user_jwt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|jwt| !jwt.is_empty()),
    })
}

/// The seed the device fingerprint is stretched from. **Never the session
/// token.** The reference derives its stable-device value from a separate
/// seed on purpose, and says why: the `info` label namespaces distinct
/// fingerprints from one seed *"so the #31 metadata and, later, the login UA
/// cannot be cross-correlated or reversed"* (`devin-connect.js`, the comment
/// above `deriveDeviceBytes`). Deriving #31 from the credential would make a
/// 366-byte value that is public-to-upstream a deterministic function of the
/// secret, and would make every deployment that pastes the same token present
/// the identical device — the opposite of what a device fingerprint is for.
///
/// The default seed is this credential's host identity, which is stable for
/// the credential's lifetime, needs nothing persisted beyond the row that
/// already exists, is not secret, and differs between two installations that
/// share one token. A credential may name its own `device_seed` instead, which
/// is what to set when a device identity has to survive re-adding the account.
fn fingerprint_seed(credential: &CredentialView<'_>) -> String {
    credential
        .secret
        .get("device_seed")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|seed| !seed.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}/{}", credential.provider_id, credential.id))
}

/// Reject a fingerprint the upstream would reject, here rather than there:
/// the server's complaint is an opaque "internal" error that looks like a
/// dead account.
pub fn validate_fingerprint(value: &str) -> Result<&str, ChannelError> {
    if value.len() != FINGERPRINT_HEX_CHARS || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ChannelError::InvalidCredential);
    }
    Ok(value)
}

/// Stretch a seed into the 366 bytes #31 wants: SHA-256 in counter mode with
/// a label, which is the HKDF expand phase the reference uses (it reaches for
/// HMAC-SHA256; the label, not the construction, is what does the work here).
/// Same seed in, same fingerprint out, with nothing persisted; a different
/// seed gives an unrelated device. The upstream checks only the shape, so the
/// construction is this channel's own choice — see [`fingerprint_seed`] for
/// what must *not* be fed to it.
pub fn derive_fingerprint(seed: &str) -> String {
    let mut out = String::with_capacity(FINGERPRINT_HEX_CHARS);
    let mut previous = [0_u8; 32];
    let mut counter = 0_u8;
    while out.len() < FINGERPRINT_HEX_CHARS {
        counter = counter.wrapping_add(1);
        let mut hasher = Sha256::new();
        hasher.update(previous);
        hasher.update(seed.as_bytes());
        hasher.update(b"devin-clientmeta");
        hasher.update([counter]);
        previous = hasher.finalize().into();
        for byte in previous {
            if out.len() < FINGERPRINT_HEX_CHARS {
                out.push_str(&format!("{byte:02x}"));
            }
        }
    }
    out
}

/// A random version-4 UUID: the per-message and per-session identifiers the
/// upstream lets the client choose.
pub(super) fn uuid() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = |slice: &[u8]| {
        slice
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    Ok(format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    ))
}

/// One encoded request plus what the response translation needs to know.
pub(super) struct Prepared {
    /// The protobuf message, before it is wrapped in a Connect envelope.
    pub body: Vec<u8>,
    /// The name the client asked for, echoed back in the response.
    pub model: String,
    /// The output cap the caller set, if any. Truncation is inferred from it
    /// rather than from the upstream's stop enum; see `stream.rs`.
    pub max_tokens: Option<u64>,
    /// The caller's stop sequences. The wire has no field for them, so they
    /// are enforced locally on the way back out; see `stream.rs`.
    pub stop: Vec<String>,
}

struct Turn {
    source: u64,
    text: String,
    images: Vec<(String, String)>,
}

pub(super) fn parse(body: &[u8]) -> Result<Value, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| super::bad_request(format!("devin request JSON: {error}")))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(super::bad_request("devin request must be an object"))
    }
}

/// OpenAI's `stop`: a bare string or up to four strings. Anything else in the
/// array is dropped, which is `normalizeStop` in
/// `samples/windsurfapi/src/stop-sequences.js`.
fn stop_sequences(request: &Value) -> Vec<String> {
    match request.get("stop") {
        Some(Value::String(one)) if !one.is_empty() => vec![one.clone()],
        Some(Value::Array(many)) => many
            .iter()
            .filter_map(Value::as_str)
            .filter(|sequence| !sequence.is_empty())
            .take(4)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// Refuse a request whose tools would be silently lost. This channel writes
/// no `ToolDef` (#10) and decodes no `delta_tool_calls` (#6): the request-side
/// inner tags and every response-side tag are uncalibrated in both mirrors,
/// and the reference keeps its own native path default-off for that reason.
/// Accepting `tools` and dropping them produces a client that waits forever
/// for a tool call it will never be sent, which is worse than a clear refusal
/// — see the module documentation's "Not implemented, and why".
fn reject_tools(request: &Value) -> Result<(), ChannelError> {
    let declared = request
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
        || request
            .get("functions")
            .and_then(Value::as_array)
            .is_some_and(|functions| !functions.is_empty());
    let chosen = match request.get("tool_choice") {
        Some(Value::String(choice)) => choice != "none",
        Some(Value::Object(_)) => true,
        _ => false,
    };
    if !declared && !chosen {
        return Ok(());
    }
    Err(super::bad_request(
        "devin: this channel does not support tool calling. The request-side \
         ToolDef tags and every response-side ChatToolCall tag are \
         uncalibrated in the mirrors this channel is built from, so declaring \
         tools here would silently drop them and no tool call would ever come \
         back. Send the request without `tools`/`tool_choice`, or route tool \
         use to another provider.",
    ))
}

pub(super) fn build(
    request: &Value,
    config: &DevinConfig,
    auth: &Auth<'_>,
) -> Result<Prepared, ChannelError> {
    let model = request
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| super::bad_request("devin request has no model"))?;
    let selector = models::resolve(model, &config.models)?;
    reject_tools(request)?;
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .filter(|messages| !messages.is_empty())
        .ok_or_else(|| super::bad_request("devin request has no messages"))?;

    let mut system = String::new();
    let mut turns: Vec<Turn> = Vec::new();
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("user")
            .trim();
        let mut images = Vec::new();
        let text = content_text(message.get("content"), &mut images);
        if role == "system" || role == "developer" {
            if !text.is_empty() {
                if !system.is_empty() {
                    system.push('\n');
                }
                system.push_str(&text);
            }
            continue;
        }
        // A tool turn has no native slot on the text path, so it is folded
        // into the following user text the way the reference does when
        // native tool history is off.
        let text = if role == "tool" {
            let id = message
                .get("tool_call_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if id.is_empty() {
                format!("[tool result]: {text}")
            } else {
                format!("[tool result for {id}]: {text}")
            }
        } else {
            text
        };
        // An assistant turn with no text is dropped, images or not. Two live
        // findings say so: the Kimi workaround records 10/10 empty retries for
        // empty assistant turns (`devin-connect.js`, the exclusion above its
        // source switch), and "empty assistant turns poison upstream into
        // repeating empty turns" (`devin-connect-openai.js`). The same
        // exclusion covers image-only assistant turns — the reference keeps
        // them out explicitly ("Image-only assistants retain the legacy
        // exclusion") because admitting one puts back exactly the text-empty
        // wire frame the exclusion exists to prevent.
        if role == "assistant" && text.trim().is_empty() {
            continue;
        }
        let source = if role == "assistant" {
            SOURCE_ASSISTANT
        } else {
            SOURCE_USER
        };
        // The upstream request validator rejects a same-source run of length
        // three or more with `invalid_argument`; a run of two is tolerated —
        // the request decodes and begins processing (`devin-connect.js`, the
        // comment above its coalescing loop, verified by wire-shape
        // comparison). So merge only the turn that would make a run reach
        // three, and leave shorter runs alone: rewriting a conversation more
        // than the wire requires changes what the model is shown.
        //
        // Only text-only neighbours merge. A turn carrying images encodes to
        // a different wire shape and must stay its own message even if that
        // leaves a long run: losing an image is worse than an invalid_argument
        // the operator can see.
        let run = turns
            .iter()
            .rev()
            .take_while(|turn| turn.source == source)
            .count();
        let mergeable = run >= 2
            && images.is_empty()
            && turns
                .last()
                .is_some_and(|last| last.images.is_empty() && last.source == source);
        match turns.last_mut() {
            Some(last) if mergeable => {
                if !text.is_empty() {
                    if !last.text.is_empty() {
                        last.text.push_str("\n\n");
                    }
                    last.text.push_str(&text);
                }
            }
            _ => turns.push(Turn {
                source,
                text,
                images,
            }),
        }
    }
    if turns.is_empty() {
        return Err(super::bad_request("devin request has no usable content"));
    }

    let max_tokens = request
        .get("max_tokens")
        .or_else(|| request.get("max_completion_tokens"))
        .and_then(Value::as_u64)
        .filter(|cap| *cap > 0);

    let mut metadata = Message::new();
    metadata
        .string(META_CLIENT_NAME, &config.client_name)
        .string(META_CLIENT_VERSION, &config.client_version)
        .string(META_SESSION_TOKEN, auth.token)
        .string(META_LOCALE, &config.locale)
        .string(META_OS, &config.os)
        .string(META_VERSION_ECHO, &config.client_version)
        .string(META_CLIENT_NAME_ECHO, &config.client_name);
    if let Some(jwt) = auth.user_jwt {
        metadata.string(META_USER_JWT, jwt);
    }
    metadata.string(META_FINGERPRINT, &auth.fingerprint);

    let mut completion = Message::new();
    let temperature = request
        .get("temperature")
        .and_then(Value::as_f64)
        .unwrap_or(DEFAULT_TEMPERATURE)
        .max(MIN_TEMPERATURE);
    completion
        .varint(COMPLETION_ENABLED, 1)
        .varint(
            COMPLETION_MAX_TOKENS,
            max_tokens.unwrap_or(config.max_tokens),
        )
        .varint(COMPLETION_CONTEXT_WINDOW, config.context_window)
        .double(COMPLETION_TEMPERATURE, temperature)
        .varint(
            COMPLETION_TOP_K,
            request
                .get("top_k")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_TOP_K),
        )
        .double(
            COMPLETION_TOP_P,
            request
                .get("top_p")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_TOP_P),
        );

    // #15.1 is a per-session configuration id and #15.2 a turn counter in the
    // captured CLI session; a stateless request matches the turn-1 capture.
    let mut model_config = Message::new();
    model_config
        .string(MODEL_CONFIG_ID, &uuid()?)
        .varint(MODEL_CONFIG_TURN, 1)
        .varint(MODEL_CONFIG_CONSTANT, 4);

    let mut out = Message::new();
    out.message(REQ_CLIENT_METADATA, &metadata)
        .string(REQ_SYSTEM_PROMPT, &system);
    for turn in &turns {
        out.message(REQ_CHAT_MESSAGE, &chat_message(turn)?);
    }
    out.varint(REQ_CONSTANT_SEVEN, 5)
        .message(REQ_COMPLETION_CONFIG, &completion)
        .message(REQ_MODEL_CONFIG, &model_config)
        .string(REQ_SESSION_ID, &uuid()?)
        .varint(REQ_CONSTANT_TWENTY, 1)
        .string(REQ_MODEL_SELECTOR, &selector);
    // #22 request_id is deliberately absent: the verified turn-1 request
    // carries none, and it is a conversation-scoped id rather than a
    // per-request one from turn 2 onwards.

    Ok(Prepared {
        body: out.into_bytes(),
        model: model.to_owned(),
        max_tokens,
        stop: stop_sequences(request),
    })
}

fn chat_message(turn: &Turn) -> Result<Message, ChannelError> {
    let mut message = Message::new();
    message
        .string(MSG_UUID, &uuid()?)
        .varint(MSG_SOURCE, turn.source)
        .string(MSG_TEXT, &turn.text);
    for (data, mime) in &turn.images {
        let mut image = Message::new();
        // #10.1 carries the base64 *text*, not the decoded bytes.
        image.string(IMAGE_BASE64, data).string(IMAGE_MIME, mime);
        message.message(MSG_IMAGES, &image);
    }
    Ok(message)
}

/// Flatten OpenAI content into the single string a `ChatMessage` carries,
/// collecting inline images as it goes. Remote image URLs are not fetched.
fn content_text(value: Option<&Value>, images: &mut Vec<(String, String)>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text" | "input_text") => {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            parts.push(text.to_owned());
                        }
                    }
                    Some("image_url" | "input_image") => {
                        let url = block
                            .pointer("/image_url/url")
                            .or_else(|| block.get("image_url"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if let Some(image) = data_url(url) {
                            images.push(image);
                        }
                    }
                    _ => {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            parts.push(text.to_owned());
                        }
                    }
                }
            }
            parts.join("\n")
        }
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `data:image/png;base64,…` to the pair `ImageData` wants. The payload is
/// validated as base64 but kept in its text form, which is what #10.1 takes.
fn data_url(url: &str) -> Option<(String, String)> {
    let rest = url.trim().strip_prefix("data:")?;
    let (media, data) = rest.split_once(",")?;
    let media = media.strip_suffix(";base64")?;
    if !media.starts_with("image/") {
        return None;
    }
    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&data)
        .ok()?;
    Some((data, media.to_owned()))
}
