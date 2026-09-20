//! The last rung of the ladder: a fingerprint of the conversation's stable
//! prefix, for clients that send no session identifier at all.
//!
//! Only the system/developer instructions and whatever precedes the first user
//! message are hashed. That part of a conversation is fixed once it starts, so
//! the fingerprint survives every added turn — hashing the whole history would
//! mint a new "session" on each request, which is worse than having none.
//!
//! This is a best-effort grouping key, not an identity. It is derived from
//! content the client controls, so it is always combined with the
//! authenticated caller's scope before it decides anything.

use gproxy_core::{SessionIdentity, SessionSource};
use gproxy_protocol::Dialect;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Enough instruction text to tell conversations apart without letting a
/// single enormous system prompt dominate the work done per request.
const MAX_PREFIX_BYTES: usize = 32 * 1024;
/// Half of a SHA-256 hex digest: 128 bits, which is far more than enough to
/// keep conversations apart and short enough to read in a log line.
const DIGEST_CHARS: usize = 32;
/// A byte that cannot appear in the middle of a UTF-8 sequence, so two
/// different splits of the same characters cannot hash alike.
const SEPARATOR: char = '\u{1f}';

/// The conversation prefix fingerprint for one inbound body, or `None` when
/// the body carries no stable prefix to hash.
pub fn fingerprint(dialect: Dialect, body: &Value) -> Option<SessionIdentity> {
    let mut prefix = Prefix::default();
    // The dialect is part of the hash: the same instructions sent as Claude
    // Messages and as OpenAI Responses are two conversations.
    prefix.push(dialect.pair_dialect().id());
    let collected = match dialect.pair_dialect() {
        Dialect::OpenAi => {
            let instructions = prefix.text(body.get("instructions"));
            prefix.messages(body.get("input")) || instructions
        }
        Dialect::OpenAiChat => prefix.messages(body.get("messages")),
        Dialect::Claude => {
            let system = prefix.text(body.get("system"));
            prefix.messages(body.get("messages")) || system
        }
        Dialect::Gemini => {
            // The Code Assist and Antigravity wrappers nest the real payload.
            let root = body
                .get("request")
                .filter(|r| r.is_object())
                .unwrap_or(body);
            let mut system = false;
            for key in ["systemInstruction", "system_instruction"] {
                system |= prefix.text(root.get(key));
            }
            prefix.messages(root.get("contents")) || system
        }
        // `pair_dialect` never returns this one.
        Dialect::OpenAiResponsesWebSocket => false,
    };
    if !collected {
        return None;
    }
    let digest = Sha256::digest(prefix.buffer.as_bytes());
    let mut id = String::with_capacity(DIGEST_CHARS);
    for byte in digest.iter().take(DIGEST_CHARS / 2) {
        use std::fmt::Write;
        let _ = write!(id, "{byte:02x}");
    }
    Some(SessionIdentity {
        id,
        source: SessionSource::ConversationFingerprint,
        field: Some("conversation.prefix".into()),
        agent_session_id: None,
    })
}

#[derive(Default)]
struct Prefix {
    buffer: String,
}

impl Prefix {
    fn push(&mut self, text: &str) {
        if text.is_empty() || self.buffer.len() >= MAX_PREFIX_BYTES {
            return;
        }
        self.buffer.push(SEPARATOR);
        let room = MAX_PREFIX_BYTES - self.buffer.len();
        if text.len() <= room {
            self.buffer.push_str(text);
        } else {
            // Truncate on a character boundary; a split UTF-8 sequence would
            // not be a valid `str` to push.
            let mut end = room;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            self.buffer.push_str(&text[..end]);
        }
    }

    /// Collect the text of one instruction field: a plain string, or the
    /// block/part array the vendor shapes use. Returns whether anything was
    /// found.
    fn text(&mut self, value: Option<&Value>) -> bool {
        let Some(value) = value else { return false };
        let before = self.buffer.len();
        self.collect(value, 0);
        self.buffer.len() != before
    }

    /// Walk a message list, collecting system and developer turns and stopping
    /// at the first turn from anyone else — the first user message ends the
    /// stable prefix, and an assistant turn means the conversation is already
    /// under way.
    fn messages(&mut self, value: Option<&Value>) -> bool {
        let Some(items) = value.and_then(Value::as_array) else {
            return false;
        };
        let before = self.buffer.len();
        for item in items {
            match item.get("role").and_then(Value::as_str) {
                Some("system" | "developer") => {}
                _ => break,
            }
            // Responses items call it `content`, Gemini calls it `parts`.
            for key in ["content", "parts", "text"] {
                if let Some(content) = item.get(key) {
                    self.collect(content, 0);
                }
            }
        }
        self.buffer.len() != before
    }

    /// Text out of the shapes these fields actually take: a string, an array
    /// of blocks, or a block with a `text` field. Bounded to the nesting the
    /// vendor shapes use rather than walking arbitrary JSON.
    fn collect(&mut self, value: &Value, depth: u8) {
        if depth > 3 {
            return;
        }
        match value {
            Value::String(text) => self.push(text),
            Value::Array(items) => {
                for item in items {
                    self.collect(item, depth + 1);
                }
            }
            Value::Object(object) => {
                for key in ["text", "content", "parts"] {
                    if let Some(nested) = object.get(key) {
                        self.collect(nested, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }
}
