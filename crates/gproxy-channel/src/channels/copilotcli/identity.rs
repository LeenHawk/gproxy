//! What the Copilot CLI announces about itself on an inference call.
//!
//! The names are the ones Copilot CLI 1.0.77's CAPI client carries
//! (`runtime.node`: `Copilot-Integration-Id`, `Openai-Intent` with the default
//! `conversation-agent`, `X-Initiator`, `X-Interaction-Id`,
//! `X-Client-Machine-Id`; `GITHUB_COPILOT_INTEGRATION_ID
//! copilot-developer-cli`). It sends no `Editor-Version` and no
//! `X-GitHub-Api-Version` on inference, so neither is sent here. The machine
//! id is derived from the credential and the interaction id is fresh per
//! request. `x-initiator` is the one value read from the body: a
//! conversation that already contains an assistant or tool turn is the agent
//! talking to itself, not a person.

use super::auth;
use super::config::{CLI_USER_AGENT, INTEGRATION_ID, OPENAI_INTENT};
use crate::channel::{ChannelError, ChannelHeaders, CredentialView};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// What the CLI itself sends. The channel sets every one of them, so a
/// provider allow-list can never hide the client being impersonated.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "copilot-integration-id",
        "openai-intent",
        "user-agent",
        "x-client-machine-id",
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

/// The machine this credential claims to be, derived from its GitHub token
/// so the same credential is always the same machine (v3
/// `identity.rs::machine_id`).
fn machine_id(github_token: &str) -> String {
    uuid(Sha256::digest(format!("copilot-machine:{github_token}")))
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

/// The GitHub token as the bearer plus the CLI's identity, on a request whose
/// forwarded headers have already had the channel's own names dropped.
pub(super) fn apply(
    headers: &mut HeaderMap,
    credential: &CredentialView<'_>,
    body: Option<&[u8]>,
) -> Result<(), ChannelError> {
    let github_token = auth::github_token(credential)?;
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {github_token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(header::USER_AGENT, HeaderValue::from_static(CLI_USER_AGENT));
    for (name, value) in [
        ("copilot-integration-id", INTEGRATION_ID.to_owned()),
        ("openai-intent", OPENAI_INTENT.to_owned()),
        ("x-initiator", initiator(body).to_owned()),
        ("x-client-machine-id", machine_id(github_token)),
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
        assert_eq!(machine_id("github"), machine_id("github"));
        assert_ne!(machine_id("github"), machine_id("other"));
    }
}
