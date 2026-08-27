mod headers;
mod http2;
mod tls;

use gproxy_channel_api::ClientProfile;
use gproxy_core::{ConfiguredFingerprint, FingerprintOverride};
use serde_json::Value;

pub(crate) fn parse(value: Option<&Value>) -> Option<ConfiguredFingerprint> {
    value.map(|value| match parse_inner(value) {
        Ok(configured) => configured,
        Err(reason) => ConfiguredFingerprint::Invalid(reason),
    })
}

fn parse_inner(value: &Value) -> Result<ConfiguredFingerprint, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "fingerprint must be a JSON object".to_owned())?;
    let headers = headers::parse(object.get("headers"))?;
    let mut profile = ClientProfile::default();
    tls::apply(object.get("tls"), &mut profile)?;
    http2::apply(object.get("http2"), &mut profile)?;
    let profile = profile.is_usable().then_some(profile);
    if headers.is_empty() && profile.is_none() {
        return Err("fingerprint contains no usable header or transport layer".into());
    }
    Ok(ConfiguredFingerprint::Usable(Box::new(
        FingerprintOverride { headers, profile },
    )))
}
