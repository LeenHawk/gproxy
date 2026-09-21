//! Who the request says it is: the account the token belongs to, and the
//! plugin the traffic claims to come from.
//!
//! WorkBuddy authenticates with a bearer token *and* with the account facts
//! beside it: `x-user-id` is mandatory, and an enterprise seat additionally
//! announces its tenant and department (v3 `auth.rs`). Those facts are
//! learned by the login and live in the credential's metadata; `prepare`
//! reads them back and never goes looking.
//!
//! The identity block is the plugin's own (v3 `identity.rs`). Three of its
//! ids are per-request — `design/session-identity.md` names them explicitly as
//! *not* session ids — and are minted fresh every time. The fourth,
//! `x-conversation-id`, is the session id that document's ladder reads, so a
//! client that sends one keeps it and only a client that sends none gets a
//! fresh one.

use super::config::WorkBuddyConfig;
use crate::channel::{ChannelError, CredentialView, HeaderAllowlist};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

/// The session id WorkBuddy's own client threads through a conversation.
pub(super) const SESSION_HEADER: &str = "x-conversation-id";

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

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(())
}

/// The bearer token and the account facts the upstream matches it against.
pub(super) fn authorize(
    headers: &mut HeaderMap,
    credential: &CredentialView<'_>,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token(credential)?))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    // The upstream rejects a token whose account it is not told; the login
    // records the id, so a credential without one is incomplete.
    let user_id = fact(credential, "user_id").ok_or(ChannelError::InvalidCredential)?;
    insert(headers, "x-user-id", user_id)?;
    for (fact_name, header_names) in [
        ("enterprise_id", &["x-enterprise-id", "x-tenant-id"][..]),
        ("department_full_name", &["x-department-info"][..]),
        ("domain", &["x-domain"][..]),
    ] {
        if let Some(value) = fact(credential, fact_name) {
            for name in header_names {
                insert(headers, name, value)?;
            }
        }
    }
    Ok(())
}

/// A version-4 UUID without its hyphens, the shape the plugin's ids have
/// (v3 `identity.rs::uuid`).
fn request_id() -> Result<String, ChannelError> {
    use std::fmt::Write as _;
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(&mut out, "{byte:02x}");
        out
    }))
}

/// The client's own session id, if it sent one an allow-list did not hide.
pub(super) fn client_session<'a>(
    headers: &'a HeaderMap,
    allowlist: Option<&HeaderAllowlist>,
) -> Option<&'a str> {
    let name = HeaderName::from_static(SESSION_HEADER);
    if allowlist.is_some_and(|list| !list.allows(&name)) {
        return None;
    }
    headers
        .get(&name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The plugin identity block. `session` is the client's conversation id when
/// it sent one; the request, message and conversation-request ids are minted
/// per call because that is what they are.
pub(super) fn identity(
    headers: &mut HeaderMap,
    config: &WorkBuddyConfig,
    session: Option<&str>,
) -> Result<(), ChannelError> {
    let request = request_id()?;
    let conversation = match session {
        Some(session) => session.to_owned(),
        None => request_id()?,
    };
    insert(headers, "x-request-id", &request)?;
    insert(headers, "x-conversation-message-id", &request)?;
    insert(headers, "x-conversation-request-id", &request)?;
    insert(headers, SESSION_HEADER, &conversation)?;
    insert(headers, "x-agent-intent", &config.agent_intent)?;
    insert(headers, "x-product", "SaaS")?;
    insert(headers, "x-ide-type", &config.ide_type)?;
    insert(headers, "x-ide-name", &config.ide_name)?;
    insert(headers, "x-ide-version", config.ide_version())?;
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_str(&config.user_agent()).map_err(|_| {
            ChannelError::InvalidConfig("WorkBuddy: `user_agent` is not a header value".into())
        })?,
    );
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid_header(name))?,
            HeaderValue::from_str(value).map_err(|_| invalid_header(name))?,
        );
    }
    Ok(())
}

fn invalid_header(name: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!("WorkBuddy: `{name}` is not a header"))
}

#[cfg(test)]
mod tests {
    use super::super::config::CLI_VERSION;
    use super::*;

    #[test]
    fn a_request_id_is_a_hyphenless_v4_uuid_and_changes_per_call() {
        let id = request_id().unwrap();
        assert_eq!(id.len(), 32);
        assert_eq!(id.as_bytes()[12], b'4');
        assert!(matches!(id.as_bytes()[16], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, request_id().unwrap());
    }

    #[test]
    fn the_conversation_id_is_the_clients_when_it_sent_one() {
        let config = WorkBuddyConfig::default();
        let mut headers = HeaderMap::new();
        identity(&mut headers, &config, Some("sess-42")).unwrap();
        assert_eq!(headers[SESSION_HEADER], "sess-42");
        assert_ne!(
            headers["x-request-id"], "sess-42",
            "a request id is not a session id"
        );

        let mut minted = HeaderMap::new();
        identity(&mut minted, &config, None).unwrap();
        assert_eq!(minted[SESSION_HEADER].to_str().unwrap().len(), 32);
        assert_eq!(minted["x-ide-version"], CLI_VERSION);
        assert_eq!(minted[header::USER_AGENT], "WorkBuddy/4.22.16");
    }
}
