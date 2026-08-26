use gproxy_channel_api::ChannelError;
use http::Uri;

const DEFAULT_URL: &str = "wss://api.openai.com/v1/realtime";

pub(crate) fn call_id(headers: &http::HeaderMap) -> Result<String, ChannelError> {
    let location = headers
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| observe("Realtime call response is missing Location"))?;
    let uri = location
        .parse::<Uri>()
        .map_err(|error| observe(format!("invalid Realtime Location: {error}")))?;
    let id = uri
        .path()
        .rsplit('/')
        .find(|part| !part.is_empty())
        .filter(|id| {
            id.starts_with("rtc_")
                && id.len() <= 256
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'))
        })
        .ok_or_else(|| observe("Realtime Location has no valid rtc_ call id"))?;
    Ok(id.into())
}

pub(crate) fn sideband_uri(call_id: &str) -> Result<Uri, ChannelError> {
    format!("{DEFAULT_URL}?call_id={call_id}")
        .parse::<Uri>()
        .map_err(|error| observe(format!("bad Realtime sideband URL: {error}")))
}

fn observe(message: impl Into<String>) -> ChannelError {
    ChannelError::Observe(message.into())
}
