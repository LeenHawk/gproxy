//! What the Copilot CLI announces about itself on an inference call.
//!
//! Every value here comes from v3 `copilotcli/identity.rs`, which captured
//! the CLI's own request: the integration it is billed as, the editor it
//! names itself as, the intent it declares, the Copilot-side API date, and
//! two ids — a stable machine id derived from the credential and a fresh
//! interaction id per request. `x-initiator` is the one value read from the
//! body: a conversation that already contains an assistant or tool turn is
//! the agent talking to itself, not a person.

use super::auth;
use super::config::{
    CLI_EDITOR_VERSION, CLI_USER_AGENT, COPILOT_API_VERSION, INTEGRATION_ID, OPENAI_INTENT,
};
use crate::channel::{ChannelError, ChannelHeaders, CredentialView};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// What the CLI itself sends. The channel sets every one of them, so a
/// provider allow-list can never hide the client being impersonated.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "copilot-integration-id",
        "editor-version",
        "openai-intent",
        "user-agent",
        "x-client-machine-id",
        "x-github-api-version",
        "x-initiator",
        "x-interaction-id",
    ],
    prefixes: &[],
};

/// Header names the channel owns; a client may not supply its own.
pub(super) const CHANNEL_HEADERS: &[&str] = CLI_HEADERS.names;

/// `user` or `agent`, from the conversation the body carries.
fn initiator(body: Option<&[u8]>) -> &'static str {
    let agent = body
        .and_then(|body| serde_json::from_slice::<Value>(body).ok())
        .and_then(|body| body.get("messages")?.as_array().cloned())
        .is_some_and(|messages| {
            messages.iter().any(|message| {
                matches!(
                    message.get("role").and_then(Value::as_str),
                    Some("assistant" | "tool")
                )
            })
        });
    if agent { "agent" } else { "user" }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(&mut out, "{byte:02x}");
        out
    })
}

/// A version-4 UUID laid over the first sixteen bytes it is given.
fn uuid(source: impl AsRef<[u8]>) -> String {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&source.as_ref()[..16]);
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

/// The machine this credential claims to be. Derived from the GitHub token,
/// which survives every Copilot-token rotation, so the same credential is
/// always the same machine (v3 `identity.rs::machine_id`).
fn machine_id(credential: &CredentialView<'_>) -> Result<String, ChannelError> {
    let seed = auth::github_token(credential)
        .or_else(|_| auth::copilot_token(credential))
        .map_err(|_| ChannelError::InvalidCredential)?;
    Ok(uuid(Sha256::digest(format!("copilot-machine:{seed}"))))
}

/// A fresh id for this exchange.
fn interaction_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        ChannelError::InvalidConfig(format!(
            "operating-system randomness is unavailable: {error}"
        ))
    })?;
    Ok(uuid(bytes))
}

/// The Copilot bearer plus the CLI's identity, on a request whose forwarded
/// headers have already had the channel's own names dropped.
pub(super) fn apply(
    headers: &mut HeaderMap,
    credential: &CredentialView<'_>,
    body: Option<&[u8]>,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", auth::copilot_token(credential)?))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(header::USER_AGENT, HeaderValue::from_static(CLI_USER_AGENT));
    for (name, value) in [
        ("copilot-integration-id", INTEGRATION_ID.to_owned()),
        ("editor-version", CLI_EDITOR_VERSION.to_owned()),
        ("openai-intent", OPENAI_INTENT.to_owned()),
        ("x-github-api-version", COPILOT_API_VERSION.to_owned()),
        ("x-initiator", initiator(body).to_owned()),
        ("x-client-machine-id", machine_id(credential)?),
        ("x-interaction-id", interaction_id()?),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(&value)
                .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_that_has_already_answered_itself_is_the_agent() {
        let user = br#"{"messages":[{"role":"user","content":"hi"}]}"#;
        assert_eq!(initiator(Some(user)), "user");
        let agent = br#"{"messages":[{"role":"assistant","content":"call"}]}"#;
        assert_eq!(initiator(Some(agent)), "agent");
        let tool = br#"{"messages":[{"role":"tool","content":"result"}]}"#;
        assert_eq!(initiator(Some(tool)), "agent");
        // A streamed request body is never parsed, so it is a person asking.
        assert_eq!(initiator(None), "user");
        assert_eq!(initiator(Some(b"not json")), "user");
    }

    #[test]
    fn the_two_ids_are_uuids_and_only_one_of_them_moves() {
        let id = interaction_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, interaction_id().unwrap());
        let machine = uuid(Sha256::digest("copilot-machine:github"));
        assert_eq!(machine, uuid(Sha256::digest("copilot-machine:github")));
        assert_ne!(machine, uuid(Sha256::digest("copilot-machine:other")));
    }
}
