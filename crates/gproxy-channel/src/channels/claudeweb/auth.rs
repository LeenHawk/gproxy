//! The browser session: which secret fields identify it and which headers
//! claude.ai's own front end sends with every call (v3 `claudeweb/auth.rs`
//! and `shared/claude/cookie.rs`).

use crate::channel::{ChannelError, CredentialView, ProviderView};
use http::{
    HeaderMap, HeaderName, HeaderValue,
    header::{ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, COOKIE, ORIGIN, REFERER},
};
use serde_json::Value;

pub const DEFAULT_BASE_URL: &str = "https://claude.ai";
/// v3 re-validated the cookie against `/api/bootstrap` every twelve hours.
pub const VALIDATION_SECS: i64 = 12 * 60 * 60;

/// Capability strings that mark a paid organization; extended thinking
/// (`paprika_mode`) is only accepted for those (v3 `auth.rs`).
const PAID_TIERS: &[&str] = &["pro", "max", "team", "enterprise", "raven"];

/// What the channel needs from a bound credential.
#[derive(Clone)]
pub(super) struct Auth {
    pub cookie: String,
    pub organization: String,
    pub device_id: Option<String>,
    pub pro: bool,
}

impl Auth {
    /// The cookie and organization live in the secret; the paid flag is read
    /// from the host-persisted metadata first and derived from the recorded
    /// capabilities otherwise.
    pub(super) fn read(credential: &CredentialView<'_>) -> Result<Self, ChannelError> {
        let secret = credential.secret;
        let metadata = credential.metadata;
        let cookie = field(secret, "cookie")
            .or_else(|| field(secret, "session_key"))
            .ok_or(ChannelError::InvalidCredential)?;
        let organization = field(secret, "organization_uuid")
            .or_else(|| field(secret, "account_uuid"))
            .or_else(|| field(metadata, "organization_uuid"))
            .ok_or(ChannelError::InvalidCredential)?;
        let pro = metadata
            .get("pro")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| {
                is_paid(secret.get("capabilities")) || is_paid(metadata.get("capabilities"))
            });
        Ok(Self {
            cookie: cookie.to_owned(),
            organization: organization.to_owned(),
            device_id: field(secret, "device_id").map(str::to_owned),
            pro,
        })
    }
}

pub(super) fn is_paid(capabilities: Option<&Value>) -> bool {
    capabilities
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .any(|value| PAID_TIERS.iter().any(|tier| value.contains(tier)))
        })
}

/// The provider's claude.ai origin without a trailing slash.
pub(super) fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

/// Headers the claude.ai front end sends: the session cookie, same-origin
/// `origin`/`referer`, `anthropic-client-platform: web_claude_ai` and, when
/// the credential carries one, the device id both as a header and as an
/// extra cookie (v3 `auth.rs::browser_headers`).
pub(super) fn browser_headers(
    cookie: &str,
    device_id: Option<&str>,
    base: &str,
    referer: &str,
) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    let mut cookie = cookie_header(cookie);
    if let Some(device) = device_id {
        headers.insert(
            HeaderName::from_static("anthropic-device-id"),
            header_value(device)?,
        );
        cookie = format!("{cookie}; anthropic-device-id={device}");
    }
    headers.insert(COOKIE, header_value(&cookie)?);
    headers.insert(ORIGIN, header_value(base)?);
    headers.insert(REFERER, header_value(referer)?);
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        HeaderName::from_static("anthropic-client-platform"),
        HeaderValue::from_static("web_claude_ai"),
    );
    Ok(headers)
}

pub(super) fn json_accept(headers: &mut HeaderMap) {
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
}

/// Headers the channel owns; a client must not be able to supply them.
pub(super) const CHANNEL_HEADERS: &[&str] = &[
    "cookie",
    "origin",
    "referer",
    "accept",
    "accept-language",
    "cache-control",
    "user-agent",
    "anthropic-device-id",
    "anthropic-client-platform",
    "anthropic-version",
    "anthropic-beta",
];

fn cookie_header(cookie: &str) -> String {
    if cookie.contains("sessionKey=") {
        cookie.to_owned()
    } else {
        format!("sessionKey={cookie}")
    }
}

/// Accept a pasted `Cookie:` header, a `k=v; k=v` cookie string or a bare
/// `sk-ant-sid...` key and return the cookie string to send
/// (v3 `shared/claude/cookie.rs`).
pub(super) fn normalize_cookie(input: &str) -> Option<String> {
    let mut text = input.trim();
    if let Some((name, value)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = value.trim();
    }
    let session_key = text.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("sessionKey=")
            .map(str::trim)
            .filter(|value| value.starts_with("sk-ant-sid"))
    });
    let session_key = session_key.or_else(|| {
        (text.starts_with("sk-ant-sid") && !text.contains(['=', ';'])).then_some(text)
    })?;
    if !text.contains("sessionKey=") {
        return Some(format!("sessionKey={session_key}"));
    }
    let pairs = text
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && part.contains('='))
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}
