//! Towards one MCP server over streamable HTTP.
//!
//! The current revision (2026-07-28) is stateless: no handshake, every
//! request carries its own context ([`stateless`]). Servers that do not
//! speak it are opened with `initialize` and addressed through a session id
//! ([`session_based`]). [`open`] finds out which a server is; the answer is a
//! [`Mode`] the host keeps, so later requests need not ask again.
//!
//! A request is answered whole: a server that streams its answer is read to
//! the response, and the notifications before it are dropped.

mod session_based;
mod stateless;
mod wire;

use gproxy_client::OutboundClient;
use gproxy_protocol::capability::CapabilityError;
use http::{HeaderMap, StatusCode};
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) use stateless::VERSION as STATELESS_VERSION;

/// One server with one credential's headers.
#[derive(Clone, Copy)]
pub struct Target<'a> {
    pub client: &'a dyn OutboundClient,
    pub endpoint: &'a str,
    /// The provider's fixed headers and the credential's.
    pub headers: &'a HeaderMap,
}

/// How a server is spoken to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "revision", rename_all = "snake_case")]
pub enum Mode {
    Stateless,
    /// `id` is the server's `Mcp-Session-Id`; a server may run sessions
    /// without issuing one.
    Session {
        id: Option<String>,
        version: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// The server answered 401. Its challenge is dropped here on purpose:
    /// what to do about it is the gateway's business, never the caller's.
    #[error("the server refused the credential")]
    Unauthorized,
    /// The server answered 404 for a session id: it ended the session.
    #[error("the server no longer knows the session")]
    SessionExpired,
    #[error("the server answered {0}")]
    Status(StatusCode),
    #[error("invalid answer from the server: {0}")]
    Invalid(String),
    #[error(transparent)]
    Transport(#[from] CapabilityError),
}

/// A JSON-RPC answer: the `result`, or the server's `error` object.
pub type Answer = Result<Value, Value>;

/// What the gateway says of itself, and of the caller it speaks for.
pub struct Context<'a> {
    /// The caller's own client capabilities, when its request carried them.
    /// A stateless server reads them to decide what it may ask for.
    pub capabilities: Option<&'a Value>,
}

fn client_info() -> Value {
    json!({ "name": "gproxy", "version": env!("CARGO_PKG_VERSION") })
}

/// Find out how the server is spoken to, opening a session when it needs one.
pub async fn open(target: Target<'_>) -> Result<Mode, CallError> {
    if stateless::supported(target).await? {
        return Ok(Mode::Stateless);
    }
    session_based::initialize(target).await
}

/// One request and its answer.
pub async fn request(
    target: Target<'_>,
    mode: &Mode,
    context: &Context<'_>,
    method: &str,
    params: Value,
) -> Result<Answer, CallError> {
    match mode {
        Mode::Stateless => stateless::request(target, context, method, params).await,
        Mode::Session { id, version } => {
            session_based::request(target, id.as_deref(), version, method, params).await
        }
    }
}

/// End a session the gateway opened. A stateless server has nothing to end.
pub async fn close(target: Target<'_>, mode: &Mode) {
    if let Mode::Session {
        id: Some(id),
        version,
    } = mode
    {
        session_based::terminate(target, id, version).await;
    }
}

/// One page of a `tools/list` result: its tools, and the cursor of the next
/// page when there is one.
pub fn tools_page(result: &Value) -> Result<(Vec<Tool>, Option<String>), CallError> {
    let tools = serde_json::from_value(result.get("tools").cloned().unwrap_or_default())
        .map_err(|error| CallError::Invalid(format!("tools/list: {error}")))?;
    let next = result
        .get("nextCursor")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok((tools, next))
}
