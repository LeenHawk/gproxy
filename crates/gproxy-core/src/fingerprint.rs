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
        return Ok(());
    }
    prepared.profile = None;
    apply_request(&mut prepared.request, provider)
}

pub(crate) fn apply_request(
    request: &mut http::Request<Bytes>,
    provider: &ProviderRef,
) -> Result<(), CoreError> {
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
                    .insert(gproxy_channel_api::ConfiguredClientProfile);
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
