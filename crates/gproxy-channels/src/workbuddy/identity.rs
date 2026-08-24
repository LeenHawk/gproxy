use gproxy_channel_api::ChannelError;
use http::header::{HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

pub(super) fn apply(headers: &mut http::HeaderMap, secret: &Value) -> Result<(), ChannelError> {
    super::auth::field(secret, "user_id").expect("auth validated WorkBuddy user_id");
    let request = uuid()?;
    let conversation = uuid()?;
    for (name, value) in [
        ("x-request-id", request.as_str()),
        ("x-conversation-message-id", request.as_str()),
        ("x-conversation-request-id", request.as_str()),
        ("x-conversation-id", conversation.as_str()),
        ("x-agent-intent", "craft"),
        ("x-product", "SaaS"),
        ("x-ide-type", "CLI"),
        ("x-ide-name", "CLI"),
        ("x-ide-version", "4.22.16"),
    ] {
        insert(headers, name, value)?;
    }
    headers.insert(USER_AGENT, HeaderValue::from_static("WorkBuddy/4.22.16"));
    Ok(())
}

fn uuid() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::Prepare("WorkBuddy request id randomness failed".into()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn insert(
    headers: &mut http::HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Prepare(format!("WorkBuddy header: {error}")))?,
    );
    Ok(())
}
