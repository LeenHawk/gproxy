//! The per-request identity the Codex CLI sends with a Responses call, for
//! clients that are not the CLI (bodies core converted from the Chat, Claude
//! or Gemini dialects). Wire facts follow `samples/codex`:
//! `core/src/client.rs` (header assembly), `core/src/responses_metadata.rs`
//! (`x-codex-turn-metadata` payload), `core/src/session/mod.rs`
//! (`current_window`: the window id is `{thread_id}:{window_number}` with the
//! number starting at 0 and a separate v7 UUID as `context_window_id`) and
//! `core/src/installation_id.rs` (a persisted UUID).
//!
//! Every id is derived, not random: a SHA-256 of a seed folded into a
//! UUID-shaped string, so two proxy instances serving the same credential
//! agree without talking, and the only things that need channel state are
//! the facts that change over time (the window number after a compaction,
//! the server's turn-state token).

use super::agent::CLI_VERSION;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Channel-state key of the credential's installation id; written once,
/// never expires, so it survives token rotation.
pub(super) const INSTALLATION_KEY: &str = "installation_id";
/// A thread's window number lives a day past its last compaction; an idle
/// thread restarting at window 0 is harmless.
pub(super) const WINDOW_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// The server's sticky-routing token is per turn; an hour outlives any tool
/// loop it could still matter to.
pub(super) const TURN_STATE_TTL_MS: i64 = 60 * 60 * 1000;

/// The `agent_name` of a plain (non-subagent) CLI thread.
const ROOT_AGENT: &str = "/root";

pub(super) fn window_key(thread_id: &str) -> String {
    format!("thread:{thread_id}:window")
}

pub(super) fn turn_state_key(thread_id: &str, turn_id: &str) -> String {
    format!("thread:{thread_id}:turn:{turn_id}:state")
}

/// What the request is for, in the CLI's `request_kind` vocabulary. The
/// channel only sees the two kinds a client can ask for; `prewarm` is a
/// CLI-internal warm-up and `memory` requests carry no identity at all
/// (`summarize_memories` in `client.rs` sends only `originator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestKind {
    Turn,
    /// A client-initiated `/responses/compact`, which the CLI tags as a
    /// manual, user-requested, standalone compaction.
    Compaction,
}

/// One request's identity. `kind` is `None` for a WebSocket handshake, where
/// the CLI sends the session and window but no turn metadata.
#[derive(Debug, Clone)]
pub(super) struct Identity {
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub installation_id: String,
    pub window_number: u64,
    pub turn_state: Option<String>,
    pub routing_hint: Option<String>,
    pub kind: Option<RequestKind>,
    pub streaming: bool,
}

impl Identity {
    /// `x-codex-window-id` as the CLI spells it.
    pub fn window_id(&self) -> String {
        format!("{}:{}", self.thread_id, self.window_number)
    }

    pub fn context_window_id(&self) -> String {
        context_window_id(&self.thread_id, self.window_number)
    }
}

// ------------------------------------------------------------ derivation

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

/// A stable, UUID-shaped id from a seed: the first sixteen SHA-256 bytes
/// with the version and variant bits of a random UUID.
pub(super) fn derived_uuid(seed: &str) -> String {
    let digest = Sha256::digest(seed);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    )
}

/// `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`, the
/// shape a persisted installation id must have to be reused.
pub(super) fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// What identifies the account for derivation: its ChatGPT account id, or
/// a digest of the access token when login recorded none.
pub(super) fn account_key(account_id: Option<&str>, access_token: &str) -> String {
    match account_id.filter(|id| !id.is_empty()) {
        Some(id) => format!("account:{id}"),
        None => format!("token:{}", hex(&Sha256::digest(access_token))),
    }
}

pub(super) fn installation_id(account_key: &str) -> String {
    derived_uuid(&format!("codex-installation:{account_key}"))
}

pub(super) fn session_id(account_key: &str, session_key: &str) -> String {
    derived_uuid(&format!("codex-session:{account_key}:{session_key}"))
}

/// A turn is one user prompt and its tool loop: the id is fixed by the
/// session and the count of user text items in `input`, so it stays the same
/// while function outputs are appended and changes with the next prompt.
pub(super) fn turn_id(session_id: &str, user_text_turns: usize) -> String {
    derived_uuid(&format!("codex-turn:{session_id}:{user_text_turns}"))
}

/// The CLI's `context_window_id` is a fresh v7 UUID per window; a derived
/// one is indistinguishable on the wire and needs no storage.
pub(super) fn context_window_id(thread_id: &str, window_number: u64) -> String {
    derived_uuid(&format!("codex-window:{thread_id}:{window_number}"))
}

// ------------------------------------------------------------------ body

/// What a Responses body says about its conversation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct BodyFacts {
    /// The session key ladder, most stable first: `prompt_cache_key`,
    /// `conversation_id`, the first user text (a conversation replayed in
    /// full keeps it), `previous_response_id` (server-side state sends only
    /// the new items, so the text ladder would churn; the root of the chain
    /// is not resolved, that needs the response body), else a fixed
    /// per-credential key.
    pub session_key: String,
    pub user_text_turns: usize,
    /// `model=<model>` or `model=<model>;tier=<service_tier>`.
    pub routing_hint: Option<String>,
}

pub(super) fn body_facts(body: &[u8]) -> BodyFacts {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return BodyFacts {
            session_key: "none".into(),
            ..BodyFacts::default()
        };
    };
    let texts = user_texts(&value);
    let string = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let session_key = if let Some(key) = string("prompt_cache_key") {
        format!("prompt_cache_key:{key}")
    } else if let Some(id) = string("conversation_id") {
        format!("conversation_id:{id}")
    } else if let Some(text) = texts.first() {
        format!("text:{}", hex(&Sha256::digest(text)))
    } else if let Some(id) = string("previous_response_id") {
        format!("previous_response_id:{id}")
    } else {
        "none".into()
    };
    let routing_hint = string("model").map(|model| match string("service_tier") {
        Some(tier) => format!("model={model};tier={tier}"),
        None => format!("model={model}"),
    });
    BodyFacts {
        session_key,
        user_text_turns: texts.len(),
        routing_hint,
    }
}

/// The text of every user message item in `input`, in order.
fn user_texts(body: &Value) -> Vec<&str> {
    match body.get("input") {
        Some(Value::String(text)) if !text.is_empty() => vec![text.as_str()],
        Some(Value::Array(items)) => items.iter().filter_map(user_text).collect(),
        _ => Vec::new(),
    }
}

fn user_text(item: &Value) -> Option<&str> {
    if item.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    if item
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind != "message")
    {
        return None;
    }
    match item.get("content")? {
        Value::String(text) => (!text.is_empty()).then_some(text.as_str()),
        Value::Array(parts) => parts.iter().find_map(|part| {
            (part.get("type").and_then(Value::as_str) == Some("input_text"))
                .then(|| part.get("text").and_then(Value::as_str))
                .flatten()
                .filter(|text| !text.is_empty())
        }),
        _ => None,
    }
}

// --------------------------------------------------------------- payload

/// `CodexTurnMetadataPayload` (`responses_metadata.rs`), the fields a plain
/// user thread fills: no subagent lineage, no fork, and the workspace,
/// sandbox and tool inventory omitted (the CLI omits them when empty).
#[derive(Serialize)]
struct TurnMetadata<'a> {
    installation_id: &'a str,
    session_id: &'a str,
    thread_id: &'a str,
    agent_name: &'static str,
    turn_id: &'a str,
    window_id: &'a str,
    window_number: u64,
    context_window_id: &'a str,
    request_kind: &'static str,
    thread_source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    compaction: Option<Compaction>,
}

/// `CompactionTurnMetadata` as a client-initiated `/responses/compact`
/// carries it (`compact_remote.rs`, the manual path).
#[derive(Serialize)]
struct Compaction {
    trigger: &'static str,
    reason: &'static str,
    implementation: &'static str,
    phase: &'static str,
    strategy: &'static str,
}

const MANUAL_COMPACTION: Compaction = Compaction {
    trigger: "manual",
    reason: "user_requested",
    implementation: "responses_compact",
    phase: "standalone_turn",
    strategy: "memento",
};

/// The `x-codex-turn-metadata` value: ASCII JSON, as the CLI's
/// `to_ascii_json_string` produces (non-ASCII escaped as `\uXXXX`).
pub(super) fn turn_metadata(identity: &Identity) -> Option<String> {
    let kind = identity.kind?;
    let window_id = identity.window_id();
    let context_window_id = identity.context_window_id();
    let payload = TurnMetadata {
        installation_id: &identity.installation_id,
        session_id: &identity.session_id,
        thread_id: &identity.thread_id,
        agent_name: ROOT_AGENT,
        turn_id: &identity.turn_id,
        window_id: &window_id,
        window_number: identity.window_number,
        context_window_id: &context_window_id,
        request_kind: match kind {
            RequestKind::Turn => "turn",
            RequestKind::Compaction => "compaction",
        },
        thread_source: "user",
        compaction: (kind == RequestKind::Compaction).then_some(MANUAL_COMPACTION),
    };
    serde_json::to_string(&payload)
        .ok()
        .map(|text| ascii(&text))
}

fn ascii(text: &str) -> String {
    if text.is_ascii() {
        return text.to_owned();
    }
    use std::fmt::Write as _;
    let mut output = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        if ch.is_ascii() {
            output.push(ch);
        } else {
            let mut units = [0_u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                let _ = write!(&mut output, "\\u{unit:04x}");
            }
        }
    }
    output
}

// --------------------------------------------------------------- headers

/// Add the identity headers a client did not send itself. Client-supplied
/// values always win: a header present in `headers` is left alone.
pub(super) fn apply(headers: &mut HeaderMap, identity: &Identity) {
    let guardian_reviewer = headers
        .get("x-codex-guardian")
        .is_some_and(|v| v == "reviewer");
    let mut put = |name: &'static str, value: &str| {
        let name = HeaderName::from_static(name);
        if headers.contains_key(&name) {
            return;
        }
        if let Ok(value) = HeaderValue::from_str(value) {
            headers.insert(name, value);
        }
    };
    put("version", CLI_VERSION);
    put("session-id", &identity.session_id);
    put("thread-id", &identity.thread_id);
    // The CLI's request id is its thread id (`codex-api` `stream_request`
    // and `build_websocket_headers`), not a per-request nonce.
    put("x-client-request-id", &identity.thread_id);
    put("x-codex-installation-id", &identity.installation_id);
    put("x-codex-window-id", &identity.window_id());
    if let Some(metadata) = turn_metadata(identity) {
        put("x-codex-turn-metadata", &metadata);
    }
    if identity.kind.is_some() {
        if !guardian_reviewer && let Some(hint) = &identity.routing_hint {
            put("x-codex-routing-hint", hint);
        }
        if let Some(state) = &identity.turn_state {
            put("x-codex-turn-state", state);
        }
        if identity.streaming {
            put("accept", "text/event-stream");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_ladder_and_user_text_turns() {
        let facts = body_facts(br#"{"prompt_cache_key":"k","conversation_id":"c","input":"hi"}"#);
        assert_eq!(facts.session_key, "prompt_cache_key:k");
        assert_eq!(facts.user_text_turns, 1);
        let facts =
            body_facts(br#"{"conversation_id":"c","input":[{"role":"user","content":"hi"}]}"#);
        assert_eq!(facts.session_key, "conversation_id:c");
        let by_text = body_facts(
            br#"{"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},{"type":"function_call_output","call_id":"c","output":"x"},{"role":"user","content":[{"type":"input_image","image_url":"u"}]}]}"#,
        );
        assert!(by_text.session_key.starts_with("text:"));
        assert_eq!(
            by_text.user_text_turns, 1,
            "images and tool outputs are not prompts"
        );
        assert_eq!(
            body_facts(br#"{"previous_response_id":"r","input":[]}"#).session_key,
            "previous_response_id:r"
        );
        assert_eq!(body_facts(br#"{}"#).session_key, "none");
        assert_eq!(body_facts(b"not json").session_key, "none");
    }

    #[test]
    fn routing_hint_follows_model_and_tier() {
        assert_eq!(
            body_facts(br#"{"model":"gpt-5.3-codex"}"#)
                .routing_hint
                .as_deref(),
            Some("model=gpt-5.3-codex")
        );
        assert_eq!(
            body_facts(br#"{"model":"gpt-5.3-codex","service_tier":"fast"}"#)
                .routing_hint
                .as_deref(),
            Some("model=gpt-5.3-codex;tier=fast")
        );
        assert_eq!(body_facts(br#"{}"#).routing_hint, None);
    }

    #[test]
    fn derived_ids_are_uuid_shaped_and_stable() {
        let a = derived_uuid("seed");
        assert_eq!(a, derived_uuid("seed"));
        assert_ne!(a, derived_uuid("other"));
        assert!(is_uuid(&a));
        assert_eq!(&a[14..15], "4");
        assert!(matches!(&a[19..20], "8" | "9" | "a" | "b"));
        assert!(!is_uuid("not-a-uuid"));
    }

    #[test]
    fn ascii_json_escapes_non_ascii() {
        assert_eq!(ascii("{\"a\":\"é\"}"), "{\"a\":\"\\u00e9\"}");
        assert_eq!(ascii("{\"a\":\"😀\"}"), "{\"a\":\"\\ud83d\\ude00\"}");
        assert_eq!(ascii("plain"), "plain");
    }
}
