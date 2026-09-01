use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PreparedRequest};

use crate::control::{ConfiguredFingerprint, ProviderRef};
use crate::error::CoreError;

pub(crate) fn apply_prepared(
    prepared: &mut PreparedRequest,
    provider: &ProviderRef,
) -> Result<(), CoreError> {
    if provider.fingerprint.is_none() {
        prepared.apply_profile();
    } else {
        prepared.profile = None;
    }
    apply_request(&mut prepared.request, provider)
}

pub fn apply_request(
    request: &mut http::Request<Bytes>,
    provider: &ProviderRef,
) -> Result<(), CoreError> {
    if let Some(proxy_url) = &provider.proxy_url {
        request
            .extensions_mut()
            .insert(crate::control::UpstreamProxy(proxy_url.clone()));
    }
    if request
        .extensions()
        .get::<gproxy_channel_api::RequiredClientProfile>()
        .is_some()
    {
        return Ok(());
    }
    let Some(configured) = &provider.fingerprint else {
        return Ok(());
    };
    request
        .extensions_mut()
        .remove::<gproxy_channel_api::ClientProfile>();
    match configured {
        ConfiguredFingerprint::Invalid(reason) => fail(provider, reason),
        ConfiguredFingerprint::Usable(configured) => {
            let headers = &configured.headers;
            let profile = &configured.profile;
            if headers.is_empty() && profile.as_ref().is_none_or(|profile| !profile.is_usable()) {
                return fail(provider, "fingerprint contains no usable override");
            }
            for (name, value) in headers {
                request.headers_mut().insert(name, value.clone());
            }
            if let Some(profile) = profile {
                request.extensions_mut().insert(profile.clone());
                request
                    .extensions_mut()
                    .insert(gproxy_channel_api::RequiredClientProfile);
            }
            Ok(())
        }
    }
}

fn fail(provider: &ProviderRef, reason: &str) -> Result<(), CoreError> {
    tracing::warn!(
        provider_id = provider.id,
        provider = %provider.name,
        reason,
        "configured client fingerprint cannot be applied"
    );
    Err(ChannelError::Prepare(format!(
        "configured client fingerprint is unusable: {reason}"
    ))
    .into())
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use gproxy_channel_api::{ClientProfile, ClientProfilePreset, RequiredClientProfile};

    use super::*;

    #[test]
    fn required_profile_survives_provider_override_and_keeps_proxy() {
        let mut request = http::Request::new(Bytes::new());
        request
            .extensions_mut()
            .insert(ClientProfile::preset(ClientProfilePreset::Chrome148));
        request.extensions_mut().insert(RequiredClientProfile);
        let mut headers = http::HeaderMap::new();
        headers.insert("x-provider-profile", "ignored".parse().unwrap());
        let provider = ProviderRef {
            id: 1,
            name: "provider".into(),
            channel: "test".into(),
            settings: serde_json::json!({}),
            fingerprint: Some(ConfiguredFingerprint::Usable(Box::new(
                crate::control::FingerprintOverride {
                    headers,
                    profile: None,
                },
            ))),
            proxy_url: Some("http://proxy.example".into()),
            traffic_blacklist: Default::default(),
        };

        apply_request(&mut request, &provider).unwrap();

        assert!(request.headers().get("x-provider-profile").is_none());
        assert_eq!(
            request.extensions().get::<ClientProfile>().unwrap().preset,
            Some(ClientProfilePreset::Chrome148)
        );
        assert_eq!(
            request
                .extensions()
                .get::<crate::control::UpstreamProxy>()
                .unwrap()
                .0,
            "http://proxy.example"
        );
    }
}
