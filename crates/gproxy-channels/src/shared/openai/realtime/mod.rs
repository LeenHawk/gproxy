mod sideband;

pub(crate) use sideband::{call_id, hangup_uri, sideband_uri};

pub(crate) fn query(
    query: Option<&str>,
    model: &str,
) -> Result<String, gproxy_channel_api::ChannelError> {
    let pairs = form_urlencoded::parse(query.unwrap_or_default().as_bytes()).collect::<Vec<_>>();
    let call_id = pairs
        .iter()
        .find(|(key, value)| key == "call_id" && !value.is_empty());
    let mut encoded = form_urlencoded::Serializer::new(String::new());
    for (key, value) in &pairs {
        if !matches!(key.as_ref(), "model" | "call_id" | "key" | "api_key") {
            encoded.append_pair(key, value);
        }
    }
    if let Some((_, value)) = call_id {
        encoded.append_pair("call_id", value);
    } else if !model.is_empty() {
        encoded.append_pair("model", model);
    } else {
        return Err(gproxy_channel_api::ChannelError::Prepare(
            "Realtime query requires model or call_id".into(),
        ));
    }
    Ok(encoded.finish())
}
