//! Revisions up to 2025-11-25: an `initialize` handshake opens a session,
//! and the server's `Mcp-Session-Id` addresses it from then on.
//!
//! The gateway initializes with no client capabilities, so a server has no
//! ground to send it sampling, elicitation or roots requests.

use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::{Answer, CallError, Mode, Target, client_info, wire};

/// The newest revision that still has the handshake.
pub(crate) const VERSION: &str = "2025-11-25";
const SESSION_ID: &str = "mcp-session-id";

fn headers(session: Option<&str>, version: &str) -> Result<HeaderMap, CallError> {
    let mut headers = HeaderMap::new();
    let (name, value) = wire::header("mcp-protocol-version", version)?;
    headers.insert(name, value);
    if let Some(session) = session {
        let (name, value) = wire::header(SESSION_ID, session)?;
        headers.insert(name, value);
    }
    Ok(headers)
}

pub(super) async fn initialize(target: Target<'_>) -> Result<Mode, CallError> {
    let body = wire::request_body(
        "initialize",
        json!({
            "protocolVersion": VERSION,
            "capabilities": {},
            "clientInfo": client_info(),
        }),
    );
    let reply = wire::send(target, Method::POST, HeaderMap::new(), Some(&body)).await?;
    let id = reply
        .headers
        .get(SESSION_ID)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let result = reply
        .answer()?
        .map_err(|error| CallError::Invalid(format!("initialize failed: {error}")))?;
    let version = result["protocolVersion"]
        .as_str()
        .ok_or_else(|| CallError::Invalid("initialize named no protocol version".to_owned()))?
        .to_owned();
    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    let reply = wire::send(
        target,
        Method::POST,
        headers(id.as_deref(), &version)?,
        Some(&initialized),
    )
    .await?;
    if !reply.status.is_success() {
        return Err(CallError::Status(reply.status));
    }
    Ok(Mode::Session { id, version })
}

/// A request written for the stateless revision names that revision and
/// its client in `_meta`. A session settled both in its handshake, and a
/// server refuses a request that contradicts it.
fn without_request_context(mut params: Value) -> Value {
    if let Some(meta) = params.get_mut("_meta").and_then(Value::as_object_mut) {
        for key in [
            "protocolVersion",
            "clientInfo",
            "clientCapabilities",
            "logLevel",
        ] {
            meta.remove(&format!("io.modelcontextprotocol/{key}"));
        }
    }
    params
}

pub(super) async fn request(
    target: Target<'_>,
    session: Option<&str>,
    version: &str,
    method: &str,
    params: Value,
) -> Result<Answer, CallError> {
    let reply = wire::send(
        target,
        Method::POST,
        headers(session, version)?,
        Some(&wire::request_body(method, without_request_context(params))),
    )
    .await?;
    if session.is_some() && reply.status == StatusCode::NOT_FOUND {
        return Err(CallError::SessionExpired);
    }
    reply.answer()
}

/// Tell the server the session is over. It may refuse (405) or already have
/// forgotten it; either way there is nothing left to do.
pub(super) async fn terminate(target: Target<'_>, session: &str, version: &str) {
    if let Ok(headers) = headers(Some(session), version) {
        let _ = wire::send(target, Method::DELETE, headers, None).await;
    }
}
