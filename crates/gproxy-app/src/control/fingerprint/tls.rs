use std::borrow::Cow;

use gproxy_channel_api::{Alpn, ClientProfile, TlsVersion};
use serde_json::Value;

pub(super) fn apply(value: Option<&Value>, profile: &mut ClientProfile) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value
        .as_object()
        .ok_or_else(|| "fingerprint tls must be an object".to_owned())?;
    if let Some(value) = object.get("alpn_protocols") {
        let values = value
            .as_array()
            .ok_or("tls.alpn_protocols must be an array")?;
        profile.alpn = Some(Cow::Owned(
            values
                .iter()
                .map(|value| match value.as_str() {
                    Some("http/1.1") => Ok(Alpn::Http1),
                    Some("h2") => Ok(Alpn::Http2),
                    Some("h3") => Ok(Alpn::Http3),
                    _ => Err("tls.alpn_protocols contains an unsupported value".into()),
                })
                .collect::<Result<Vec<_>, String>>()?,
        ));
    }
    profile.grease = optional_bool(object, "grease_enabled")?;
    profile.min_tls_version = optional_version(object, "min_tls_version")?;
    profile.max_tls_version = optional_version(object, "max_tls_version")?;
    profile.cipher_list = optional_string(object, "cipher_list")?.map(Cow::Owned);
    profile.curves_list = optional_string(object, "curves_list")?.map(Cow::Owned);
    profile.sigalgs_list = optional_string(object, "sigalgs_list")?.map(Cow::Owned);
    profile.preserve_tls13_cipher_list = optional_bool(object, "preserve_tls13_cipher_list")?;
    if let Some(value) = object.get("extension_permutation") {
        let values = value
            .as_array()
            .ok_or("tls.extension_permutation must be an array")?;
        let values = values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|value| u16::try_from(value).ok())
                    .ok_or_else(|| "tls.extension_permutation contains an invalid id".into())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !values.is_empty() {
            profile.extension_permutation = Some(Cow::Owned(values));
        }
    }
    Ok(())
}

fn optional_bool(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<bool>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("tls.{field} must be a boolean"))
        })
        .transpose()
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("tls.{field} must be a non-empty string"))
        })
        .transpose()
}

fn optional_version(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<TlsVersion>, String> {
    object
        .get(field)
        .map(
            |value| match value.as_str().map(str::to_ascii_lowercase).as_deref() {
                Some("tls1") | Some("tls1.0") => Ok(TlsVersion::Tls10),
                Some("tls1.1") => Ok(TlsVersion::Tls11),
                Some("tls1.2") => Ok(TlsVersion::Tls12),
                Some("tls1.3") => Ok(TlsVersion::Tls13),
                _ => Err(format!("tls.{field} is not a supported TLS version")),
            },
        )
        .transpose()
}
