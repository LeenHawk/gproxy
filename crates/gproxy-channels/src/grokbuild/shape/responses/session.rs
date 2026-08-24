use gproxy_channel_api::ChannelError;
use serde_json::{Map, Value};

const COMPOSER_PREFIX: &str = "grok-composer-";

pub(super) fn ensure(object: &mut Map<String, Value>) -> Result<(), ChannelError> {
    if object
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || !object
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .starts_with(COMPOSER_PREFIX)
    {
        return Ok(());
    }
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::Prepare("Grok session randomness failed".into()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    object.insert(
        "prompt_cache_key".into(),
        Value::String(format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        )),
    );
    Ok(())
}
