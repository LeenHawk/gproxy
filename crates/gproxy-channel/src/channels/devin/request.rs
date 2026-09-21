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
const COMPLETION_MAX_NEWLINES: u32 = 3;
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
/// plus an optional `fingerprint` and an optional short-lived `user_jwt`.
pub(super) fn auth<'a>(credential: &CredentialView<'a>) -> Result<Auth<'a>, ChannelError> {
    let token = ["session_token", "api_key", "token"]
        .into_iter()
        .find_map(|key| credential.secret.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let fingerprint = match credential.secret.get("fingerprint").and_then(Value::as_str) {
        Some(configured) => validate_fingerprint(configured.trim())?.to_owned(),
        None => derive_fingerprint(token),
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

/// Reject a fingerprint the upstream would reject, here rather than there:
/// the server's complaint is an opaque "internal" error that looks like a
/// dead account.
pub fn validate_fingerprint(value: &str) -> Result<&str, ChannelError> {
    if value.len() != FINGERPRINT_HEX_CHARS || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ChannelError::InvalidCredential);
    }
    Ok(value)
}

/// A fingerprint that is stable for one credential and different for the
/// next, without anything to persist: SHA-256 in counter mode over the
/// session token, stretched to 366 bytes. The upstream checks only the shape,
/// so the derivation is this channel's own choice; the reference derives its
/// stable-device value the same way from an HMAC (`deriveDeviceBytes`).
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
        // An assistant turn with nothing in it is dropped: the reference
        // records empty assistant turns being answered with empty replies.
        if role == "assistant" && text.trim().is_empty() && images.is_empty() {
            continue;
        }
        let source = if role == "assistant" {
            SOURCE_ASSISTANT
        } else {
            SOURCE_USER
        };
        // The upstream request validator rejects a run of three or more
        // consecutive messages from the same source with `invalid_argument`;
        // merging text-only neighbours keeps the content identical and the
        // run below that threshold.
        match turns.last_mut() {
            Some(last) if last.source == source && last.images.is_empty() && images.is_empty() => {
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
        .varint(COMPLETION_MAX_NEWLINES, config.max_newlines)
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
