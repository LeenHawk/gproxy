use gproxy_channel_api::ChannelError;
use http::header::{HeaderName, HeaderValue, USER_AGENT};

const CLI_VERSION: &str = "0.36.1";

pub(super) fn apply(headers: &mut http::HeaderMap, device_id: &str) -> Result<(), ChannelError> {
    headers.insert(USER_AGENT, HeaderValue::from_static("kimi-code-cli/0.36.1"));
    for (name, value) in [
        ("x-msh-platform", "kimi_code_cli".into()),
        ("x-msh-version", CLI_VERSION.into()),
        ("x-msh-device-name", device_name()),
        ("x-msh-device-model", device_model()),
        ("x-msh-os-version", os_version()),
        ("x-msh-device-id", device_id.into()),
    ] {
        insert(headers, name, &value)?;
    }
    Ok(())
}

fn insert(
    headers: &mut http::HeaderMap,
    name: &'static str,
    value: &str,
) -> Result<(), ChannelError> {
    let value = value
        .chars()
        .filter(|character| character.is_ascii() && !character.is_ascii_control())
        .collect::<String>();
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_str(if value.trim().is_empty() {
            "unknown"
        } else {
            value.trim()
        })
        .map_err(|error| ChannelError::Prepare(format!("Kimi identity header: {error}")))?,
    );
    Ok(())
}

fn device_name() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".into())
}

fn os_version() -> String {
    std::env::var("KERNEL_RELEASE").unwrap_or_else(|_| std::env::consts::OS.into())
}

fn device_model() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}
