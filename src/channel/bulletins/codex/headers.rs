//! Codex CLI fingerprint and authorization headers for content requests.

use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};

use crate::channel::ChannelError;

pub(super) const ORIGINATOR: &str = "codex_exec";
pub(super) const USER_AGENT_VALUE: &str =
    "codex_exec/0.144.0 (Debian 13.0.0; x86_64) xterm-256color (codex_exec; 0.144.0)";

/// Inject the OAuth bearer and Codex CLI fingerprint. Client-provided session
/// identifiers are preserved; otherwise a matching pair is generated.
pub(super) fn apply(
    request: &mut Request<Bytes>,
    access_token: &str,
    account_id: Option<&str>,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|error| ChannelError::InvalidCredential(format!("bad access_token: {error}")))?;
    let session_id = HeaderValue::from_str(&crate::util::rand::uuid_v4())
        .map_err(|error| ChannelError::Build(format!("bad session id: {error}")))?;

    let headers = request.headers_mut();
    headers.insert(AUTHORIZATION, bearer);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    headers.insert(
        HeaderName::from_static("originator"),
        HeaderValue::from_static(ORIGINATOR),
    );
    headers
        .entry(HeaderName::from_static("session-id"))
        .or_insert_with(|| session_id.clone());
    headers
        .entry(HeaderName::from_static("x-client-request-id"))
        .or_insert(session_id);
    if let Some(account_id) = account_id {
        let account_id = HeaderValue::from_str(account_id)
            .map_err(|error| ChannelError::InvalidCredential(format!("bad account_id: {error}")))?;
        headers.insert(HeaderName::from_static("chatgpt-account-id"), account_id);
    }
    Ok(())
}
