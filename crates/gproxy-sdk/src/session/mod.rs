//! Reading a conversation identity out of an inbound request.
//!
//! Implements the workspace's `design/session-identity.md`. The identity is
//! what later keeps a conversation on one credential, so it has to describe
//! the client that actually sent the request — never the upstream the request
//! is about to be forwarded to. A Claude Code request routed to a Gemini
//! provider still carries Claude Code's identity.
//!
//! The ladder, in order:
//!
//! 1. the gateway header `x-gproxy-session-id`, or a value the SDK caller set
//!    explicitly — [`SessionSource::Gateway`];
//! 2. a native client header (including `x-opencode-session`) or a v3
//!    compatibility session header;
//! 3. the native body field of the inbound shape, and only of that shape;
//! 4. a fingerprint of the stable conversation prefix;
//! 5. nothing — the caller supplies a request-level id with
//!    [`SessionSource::RequestFallback`], which is explicitly *not* a session.
//!
//! Every step is exact. Nothing recurses through arbitrary JSON looking for a
//! field that happens to be called `session_id`: a business payload that
//! carries one would otherwise merge unrelated callers onto one credential.
//!
//! A session id is not authentication. It never widens what a caller may
//! reach, and the affinity it feeds is always scoped by the authenticated
//! caller as well — see `Core`'s `AffinityScope`.

mod fingerprint;

use gproxy_core::{SessionIdentity, SessionSource};
use gproxy_protocol::{Dialect, OperationKey};
use http::HeaderMap;
use serde_json::Value;

pub use fingerprint::fingerprint;

/// The gateway's own session header. Fixed: the discussed
/// `x-gproxy-session0id` spelling is a typo and gets no alias.
pub const GATEWAY_SESSION_HEADER: &str = "x-gproxy-session-id";

/// Native client session headers, in the order they are consulted.
///
/// Codex's thread comes before its session because affinity is per
/// conversation: two threads of one session are two conversations and should
/// not be forced onto one credential.
const NATIVE_HEADERS: [(&str, SessionSource); 9] = [
    ("x-opencode-session", SessionSource::OpenCode),
    ("thread-id", SessionSource::CodexThread),
    ("session-id", SessionSource::CodexSession),
    ("x-claude-code-session-id", SessionSource::ClaudeCode),
    ("x-conversation-id", SessionSource::WorkBuddy),
    ("x-grok-session-id", SessionSource::GrokBuild),
    ("x-session-id", SessionSource::Generic),
    ("x-session-affinity", SessionSource::Generic),
    ("session_id", SessionSource::ClaudeCode),
];

/// The whole ladder over one request. `body` is the already-decoded JSON body,
/// when there is one; a request without a JSON body stops after the headers.
///
/// Returns `None` when nothing stable could be derived. The caller then uses
/// its own request id with [`SessionSource::RequestFallback`] rather than
/// claiming a cross-turn session that does not exist.
pub fn extract(
    headers: &HeaderMap,
    body: Option<&Value>,
    operation: OperationKey,
) -> Option<SessionIdentity> {
    if let Some(identity) = from_headers(headers) {
        return Some(identity);
    }
    let body = body?;
    from_body(operation.dialect, body).or_else(|| fingerprint(operation.dialect, body))
}

/// The gateway header, then the native client headers. Our own id wins over a
/// client's: the two are never concatenated.
pub fn from_headers(headers: &HeaderMap) -> Option<SessionIdentity> {
    if let Some(id) = header(headers, GATEWAY_SESSION_HEADER) {
        return Some(identity(id, SessionSource::Gateway, GATEWAY_SESSION_HEADER));
    }
    NATIVE_HEADERS
        .iter()
        .find_map(|(name, source)| Some(identity(header(headers, name)?, *source, name)))
}

/// The native session field of one inbound shape. Only the fields that shape
/// actually defines are read; a Claude body is never searched for Codex's.
pub fn from_body(dialect: Dialect, body: &Value) -> Option<SessionIdentity> {
    match dialect.pair_dialect() {
        // OpenAI Responses, which is what Codex speaks. The websocket envelope
        // resolves to this through `pair_dialect`.
        Dialect::OpenAi => {
            let metadata = body.get("client_metadata")?;
            string(metadata.get("thread_id"))
                .map(|id| identity(id, SessionSource::CodexThread, "client_metadata.thread_id"))
                .or_else(|| {
                    Some(identity(
                        string(metadata.get("session_id"))?,
                        SessionSource::CodexSession,
                        "client_metadata.session_id",
                    ))
                })
        }
        // Claude Messages. `metadata.user_id` is a JSON-encoded *string*; the
        // session lives inside it. The whole value is an account-scoped
        // identifier and must never be used as the session itself.
        Dialect::Claude => {
            let encoded = string(body.get("metadata").and_then(|m| m.get("user_id")))?;
            let decoded: Value = serde_json::from_str(&encoded).ok()?;
            Some(identity(
                string(decoded.get("session_id"))?,
                SessionSource::ClaudeCode,
                "metadata.user_id.session_id",
            ))
        }
        // Gemini: the Code Assist wrapper spells it with an underscore and
        // Antigravity's wrapper in camel case. Both are read, neither is
        // guessed from the other.
        Dialect::Gemini => {
            let request = body.get("request")?;
            string(request.get("session_id"))
                .map(|id| identity(id, SessionSource::GeminiCli, "request.session_id"))
                .or_else(|| {
                    Some(identity(
                        string(request.get("sessionId"))?,
                        SessionSource::Antigravity,
                        "request.sessionId",
                    ))
                })
        }
        // Chat Completions defines no session field. `prompt_cache_key` and
        // `previous_response_id` have their own protocol meanings and are not
        // cross-client session ids.
        Dialect::OpenAiChat => None,
        // `pair_dialect` never returns this one.
        Dialect::OpenAiResponsesWebSocket => None,
    }
}

/// Remove the gateway's own header so it is never forwarded upstream. The
/// native session fields a channel needs are the outbound adapter's business.
pub fn strip_gateway_header(headers: &mut HeaderMap) {
    headers.remove(GATEWAY_SESSION_HEADER);
}

/// A present, decodable, non-blank header value. Header names are
/// case-insensitive; the value is used exactly as sent.
fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// A present, non-blank JSON string. A number or object in a session field is
/// not coerced: the field is simply absent.
fn string(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn identity(id: String, source: SessionSource, field: &str) -> SessionIdentity {
    SessionIdentity {
        id,
        source,
        field: Some(field.to_owned()),
        agent_session_id: None,
    }
}
