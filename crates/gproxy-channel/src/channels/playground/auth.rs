use std::collections::BTreeMap;

use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::{Playground, config, request};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, OperationFuture, RefreshContext,
};

pub(super) const ORIGIN: &str = "https://aistudio.google.com";
// Public browser-client key from the captured first-party MakerSuite request.
// WAA/Create uses a different key, restricted to that service.
const WEB_API_KEY: &str = "AIzaSyDdP816MREB3SkjZO04QXbjsigfcI0GWOs";
pub(super) const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36";

fn session_cookie(name: &str) -> bool {
    matches!(
        name,
        "SID"
            | "HSID"
            | "SSID"
            | "APISID"
            | "SAPISID"
            | "SIDCC"
            | "__Secure-1PSID"
            | "__Secure-3PSID"
            | "__Secure-1PAPISID"
            | "__Secure-3PAPISID"
            | "__Secure-1PSIDTS"
            | "__Secure-3PSIDTS"
            | "__Secure-1PSIDCC"
            | "__Secure-3PSIDCC"
    )
}

/// Deliberately neither Debug nor Serialize: browser cookies are credentials.
pub(super) struct Session {
    pub cookies: BTreeMap<String, String>,
    pub auth_user: String,
}

impl Session {
    pub fn parse(secret: &Value) -> Result<Self, ChannelError> {
        let input = secret
            .get("cookies")
            .or_else(|| secret.get("cookie"))
            .ok_or(ChannelError::InvalidCredential)?;
        let mut cookies = BTreeMap::new();
        let mut insert = |name: &str, value: &str| -> Result<(), ChannelError> {
            if session_cookie(name) && !value.is_empty() {
                if value.contains(['\r', '\n', ';']) {
                    return Err(ChannelError::InvalidCredential);
                }
                cookies.insert(name.to_owned(), value.to_owned());
            }
            Ok(())
        };
        match input {
            Value::String(text) => {
                let text = text.trim();
                let text = text
                    .split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("cookie"))
                    .map_or(text, |(_, value)| value.trim());
                for pair in text.split(';') {
                    if let Some((name, value)) = pair.trim().split_once('=') {
                        insert(name.trim(), value.trim())?;
                    }
                }
            }
            Value::Array(values) => {
                for cookie in values {
                    if let Some(domain) = cookie.get("domain").and_then(Value::as_str) {
                        let domain = domain.trim_start_matches('.');
                        if domain != "google.com" && !domain.ends_with(".google.com") {
                            continue;
                        }
                    }
                    let name = cookie
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or(ChannelError::InvalidCredential)?;
                    let value = cookie
                        .get("value")
                        .and_then(Value::as_str)
                        .ok_or(ChannelError::InvalidCredential)?;
                    insert(name, value)?;
                }
            }
            Value::Object(values) => {
                for (name, value) in values {
                    insert(name, value.as_str().ok_or(ChannelError::InvalidCredential)?)?;
                }
            }
            _ => return Err(ChannelError::InvalidCredential),
        }
        if !["SAPISID", "__Secure-1PAPISID", "__Secure-3PAPISID"]
            .iter()
            .any(|key| cookies.contains_key(*key))
            || !["SID", "__Secure-1PSID", "__Secure-3PSID"]
                .iter()
                .any(|key| cookies.contains_key(*key))
        {
            return Err(ChannelError::InvalidCredential);
        }
        let auth_user = match secret.get("auth_user") {
            None => "0".into(),
            Some(Value::String(value))
                if !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()) =>
            {
                value.clone()
            }
            Some(Value::Number(value)) if value.is_u64() => value.to_string(),
            _ => return Err(ChannelError::InvalidCredential),
        };
        Ok(Self { cookies, auth_user })
    }

    pub fn cookie(&self) -> String {
        self.cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn headers(&self, tier: Option<&str>) -> Result<HeaderMap, ChannelError> {
        let timestamp = now_ms()? / 1000;
        let fallback = self
            .cookies
            .get("SAPISID")
            .or_else(|| self.cookies.get("__Secure-3PAPISID"))
            .or_else(|| self.cookies.get("__Secure-1PAPISID"))
            .ok_or(ChannelError::InvalidCredential)?;
        let authorization = [
            ("SAPISIDHASH", "SAPISID"),
            ("SAPISID1PHASH", "__Secure-1PAPISID"),
            ("SAPISID3PHASH", "__Secure-3PAPISID"),
        ]
        .into_iter()
        .map(|(label, name)| {
            let value = self.cookies.get(name).unwrap_or(fallback);
            let digest = Sha1::digest(format!("{timestamp} {value} {ORIGIN}"));
            let hex = digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            format!("{label} {timestamp}_{hex}")
        })
        .collect::<Vec<_>>()
        .join(" ");
        let mut headers = HeaderMap::new();
        for (name, value) in [
            (header::CONTENT_TYPE, "application/json+protobuf"),
            (header::ORIGIN, ORIGIN),
            (header::REFERER, "https://aistudio.google.com/"),
            (header::USER_AGENT, USER_AGENT),
            (
                header::HeaderName::from_static("x-goog-api-key"),
                WEB_API_KEY,
            ),
            (
                header::HeaderName::from_static("x-user-agent"),
                "grpc-web-javascript/0.1",
            ),
        ] {
            headers.insert(name, HeaderValue::from_static(value));
        }
        headers.insert(header::AUTHORIZATION, header_value(&authorization)?);
        headers.insert(header::COOKIE, header_value(&self.cookie())?);
        headers.insert("x-goog-authuser", header_value(&self.auth_user)?);
        if let Some(tier) = tier {
            headers.insert("x-aistudio-g1-tier", header_value(tier)?);
        }
        Ok(headers)
    }
}

pub(super) fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

pub(super) fn now_ms() -> Result<i64, ChannelError> {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .map_err(|_| ChannelError::InvalidResponse("system clock precedes Unix epoch".into()))
}

impl CredentialRefresh for Playground {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let session = Session::parse(context.credential.secret)?;
            let response = context
                .client
                .send(request::post(
                    &config::endpoint(context.provider, "GetAiStudioBenefitTier", None),
                    session.headers(None)?,
                    &json!([]),
                )?)
                .await?;
            let status = response.status;
            let bytes = crate::channel::read_body(response.body).await?;
            if status == StatusCode::UNAUTHORIZED {
                return Err(ChannelError::RefreshRejected(
                    "Google browser session expired; import fresh cookies".into(),
                ));
            }
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let body: Value = serde_json::from_slice(&bytes)
                .map_err(|_| request::invalid("invalid benefit tier response"))?;
            let tier = body
                .get(0)
                .and_then(Value::as_u64)
                .ok_or_else(|| request::invalid("missing benefit tier"))?;
            let (plan, header) = match tier {
                1 => ("Pro", "TIER1"),
                2 => ("Ultra", "TIER2"),
                3 => ("Free", "TIER0"),
                _ => ("Unknown", "BENEFIT_TIER_UNSPECIFIED"),
            };
            let mut secret = context
                .credential
                .secret
                .as_object()
                .cloned()
                .ok_or(ChannelError::InvalidCredential)?;
            secret.remove("cookies");
            secret.insert("cookie".into(), session.cookie().into());
            secret.insert("auth_user".into(), session.auth_user.into());
            let mut metadata = context
                .credential
                .metadata
                .as_object()
                .cloned()
                .unwrap_or_default();
            metadata.insert("plan".into(), plan.into());
            metadata.insert("benefit_tier".into(), header.into());
            Ok(CredentialUpdate {
                secret: secret.into(),
                metadata: Some(metadata.into()),
                expires_at_ms: Some(now_ms()? + 12 * 60 * 60 * 1000),
            })
        })
    }
}
