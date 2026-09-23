//! The bearer token and the CLI identity that goes with it (v3
//! `grokbuild/auth.rs`).
//!
//! `grok-shell` announces itself with a fixed block of `x-grok-client-*`
//! headers and an `x-xai-token-auth` marker that tells the chat proxy the
//! bearer is an OAuth account token rather than an API key. Two more headers
//! are per-account and per-turn: `x-grok-user-id` is the `sub` claim the login
//! read out of the id token, and `x-grok-conv-id` is the request body's
//! `prompt_cache_key`.
//!
//! `x-grok-conv-id` is *not* the session id. `design/session-identity.md` says
//! so explicitly: the CLI regenerates it on some auxiliary calls, and the
//! session the ladder reads is `x-grok-session-id`. That header is therefore
//! left alone here — the channel neither sets nor drops it, so a client's own
//! reaches the upstream under its own name.

use super::config::GrokBuildConfig;
use crate::channel::{ChannelError, CredentialView};
use crate::channels::shared::compatible::http::insert_configured;
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

/// The session id the ladder reads. The channel only declares it so a
/// provider allow-list cannot strip it.
pub(super) const SESSION_HEADER: &str = "x-grok-session-id";
/// The CLI's own version banner (v3 `auth.rs`).
pub const CLI_VERSION: &str = "1.0.0";
pub const CLI_USER_AGENT: &str = "grok-shell/1.0.0";

/// What the CLI sends on every call, regardless of the account.
const STATIC_HEADERS: &[(&str, &str)] = &[
    ("x-xai-token-auth", "xai-grok-cli"),
    ("x-authenticateresponse", "authenticate-response"),
    ("x-grok-client-version", CLI_VERSION),
    ("x-grok-client-identifier", "grok-shell"),
    ("x-grok-client-mode", "headless"),
];

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A public account fact: host metadata first, then the secret's
/// `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    non_empty(credential.metadata.get(name))
        .or_else(|| {
            non_empty(
                credential
                    .secret
                    .pointer(&format!("/provider_fields/{name}")),
            )
        })
        .or_else(|| non_empty(credential.secret.get(name)))
}

pub(super) fn access_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    non_empty(credential.secret.get("access_token")).ok_or(ChannelError::InvalidCredential)
}

/// What the reply is expected to look like, which decides `Accept`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reply {
    Json,
    Stream,
    Audio,
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(())
}

pub(super) fn apply(
    headers: &mut HeaderMap,
    config: &GrokBuildConfig,
    credential: &CredentialView<'_>,
    reply: Reply,
    conversation: Option<&str>,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token(credential)?))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    // A multipart upload keeps the content type the caller wrote, boundary
    // and all; only a body with none is declared JSON.
    if !headers.contains_key(header::CONTENT_TYPE) {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(match reply {
            Reply::Audio => "audio/*",
            Reply::Stream => "text/event-stream",
            Reply::Json => "application/json",
        }),
    );
    for (name, value) in STATIC_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(CLI_USER_AGENT),
    );
    if let Some(conversation) = conversation.map(str::trim).filter(|id| !id.is_empty()) {
        insert(headers, "x-grok-conv-id", conversation)?;
    }
    if let Some(user) = fact(credential, "sub") {
        insert(headers, "x-grok-user-id", user)?;
    }
    for (name, value) in &config.headers {
        insert_configured(headers, name, value)?;
    }
    Ok(())
}
