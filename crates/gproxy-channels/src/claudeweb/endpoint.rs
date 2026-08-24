use gproxy_channel_api::ChannelError;
use serde_json::Value;

pub(super) fn url(
    settings: &Value,
    base: &str,
    organization: &str,
    conversation: &str,
    key: &str,
    path: &str,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = settings
        .pointer(&format!("/endpoints/{key}"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        let url = url
            .replace(
                "{organization}",
                &crate::shared::http::encode_component(organization),
            )
            .replace(
                "{conversation}",
                &crate::shared::http::encode_component(conversation),
            );
        crate::shared::http::exact(&url, None)
    } else {
        crate::shared::http::join(base, path, None)
    }
}

pub(super) fn conversation(organization: &str, conversation: &str) -> String {
    format!("/api/organizations/{organization}/chat_conversations/{conversation}")
}
