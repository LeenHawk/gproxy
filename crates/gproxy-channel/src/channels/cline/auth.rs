//! How a Cline credential authenticates and the client identity it presents.

use crate::channel::{ChannelError, ChannelHeaders, CredentialView};
use base64::Engine as _;
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde_json::Value;

/// A login token is a WorkOS JWT and the upstream wants it announced as one;
/// a pasted API key is presented verbatim (v3 `cline/auth.rs::bearer`).
const WORKOS_PREFIX: &str = "workos:";

/// What the Cline SDK sends alongside every request: the referer and title
/// Cline attributes traffic with, and the client kind (v3
/// `cline/auth.rs::apply`). The channel sets all three, so a provider
/// allow-list can never hide the client being impersonated.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["http-referer", "x-title", "x-client-type"],
    prefixes: &[],
};

/// Header names the channel owns; a client may not supply its own.
pub(super) const CHANNEL_HEADERS: &[&str] = &["http-referer", "x-title", "x-client-type"];

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A public account fact the login recorded: host metadata first, then the
/// secret's `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    text(credential.metadata.get(name))
        .or_else(|| {
            text(
                credential
                    .secret
                    .pointer(&format!("/provider_fields/{name}")),
            )
        })
        .or_else(|| text(credential.secret.get(name)))
}

/// The bearer this credential presents. A login token carries the `workos:`
/// scheme the upstream keys account traffic on; a pasted `api_key` does not.
pub(super) fn bearer(credential: &CredentialView<'_>) -> Result<String, ChannelError> {
    if let Some(token) = text(credential.secret.get("access_token")) {
        return Ok(if token.to_ascii_lowercase().starts_with(WORKOS_PREFIX) {
            token.to_owned()
        } else {
            format!("{WORKOS_PREFIX}{token}")
        });
    }
    text(credential.secret.get("api_key"))
        .map(str::to_owned)
        .ok_or(ChannelError::InvalidCredential)
}

/// A pasted key, when the credential has one. The plan-usage probe presents
/// it verbatim rather than as an account token (v3 `cline/quota.rs`).
pub(super) fn api_key<'a>(credential: &CredentialView<'a>) -> Option<&'a str> {
    text(credential.secret.get("api_key"))
}

/// The expiry encoded in a WorkOS access token: its JWT payload's `exp`, in
/// milliseconds (v3 `cline/auth.rs::token_expiry`). Nothing else on the wire
/// dates the token, so a credential whose token is not a readable JWT simply
/// has no expiry and the host refreshes it when the upstream refuses it.
pub(super) fn token_expires_at_ms(token: &str) -> Option<i64> {
    let token = token.strip_prefix(WORKOS_PREFIX).unwrap_or(token);
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get("exp")?
        .as_i64()?
        .checked_mul(1_000)
}

fn insert(headers: &mut HeaderMap, name: HeaderName, value: &str) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)?,
    );
    Ok(())
}

/// The bearer plus the SDK's attribution headers, on traffic and on the
/// account probes alike.
pub(super) fn apply(
    headers: &mut HeaderMap,
    credential: &CredentialView<'_>,
) -> Result<(), ChannelError> {
    insert(
        headers,
        header::AUTHORIZATION,
        &format!("Bearer {}", bearer(credential)?),
    )?;
    for (name, value) in [
        ("http-referer", "https://cline.bot"),
        ("x-title", "Cline"),
        ("x-client-type", "cline-sdk"),
    ] {
        insert(headers, HeaderName::from_static(name), value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_account_token_is_announced_as_one_and_is_never_announced_twice() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"exp":1800000000,"sub":"u"}"#);
        let jwt = format!("header.{payload}.signature");
        assert_eq!(token_expires_at_ms(&jwt), Some(1_800_000_000_000));
        assert_eq!(
            token_expires_at_ms(&format!("{WORKOS_PREFIX}{jwt}")),
            Some(1_800_000_000_000)
        );
        assert_eq!(token_expires_at_ms("not-a-jwt"), None);
    }
}
