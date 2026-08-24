use gproxy_channel_api::ChannelError;
use http::header::{CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;
use sha2::{Digest, Sha256};

const CLI_USER_AGENT: &str = "copilot/1.0.61 (linux v24.16.0) term/unknown";

pub(super) fn apply(
    headers: &mut http::HeaderMap,
    secret: &Value,
    body: &[u8],
) -> Result<(), ChannelError> {
    super::auth::insert_bearer(headers, super::auth::copilot_token(secret)?)?;
    let machine = machine_id(secret);
    for (name, value) in [
        ("copilot-integration-id", "copilot-developer-cli"),
        ("editor-version", "copilot/1.0.61"),
        ("openai-intent", "conversation-agent"),
        ("x-github-api-version", "2026-06-01"),
        ("x-initiator", initiator(body)),
    ] {
        insert(headers, name, value)?;
    }
    insert(headers, "x-client-machine-id", &machine)?;
    insert(headers, "x-interaction-id", &interaction_id()?)?;
    headers.insert(USER_AGENT, HeaderValue::from_static(CLI_USER_AGENT));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(())
}

fn initiator(body: &[u8]) -> &'static str {
    let agent = serde_json::from_slice::<Value>(body)
        .ok()
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

fn machine_id(secret: &Value) -> String {
    let seed = super::auth::field(secret, "github_token")
        .or_else(|| super::auth::field(secret, "copilot_token"))
        .expect("Copilot auth validated a token");
    uuid(Sha256::digest(format!("copilot-machine:{seed}")))
}

fn interaction_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::Prepare("Copilot interaction id randomness failed".into()))?;
    Ok(uuid(bytes))
}

fn uuid(digest: impl AsRef<[u8]>) -> String {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn insert(
    headers: &mut http::HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value).map_err(|error| {
            ChannelError::Prepare(format!("Copilot header is invalid: {error}"))
        })?,
    );
    Ok(())
}
