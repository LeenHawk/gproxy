use std::collections::BTreeMap;

use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::{Playground, config, request};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, CredentialView, OperationFuture,
    RefreshContext, RefreshReason,
};
use gproxy_client::OutboundClient;
use gproxy_protocol::HttpBody;

pub(super) const ORIGIN: &str = "https://aistudio.google.com";
// Public browser-client key from the captured first-party MakerSuite request.
// WAA/Create uses a different key, restricted to that service.
const WEB_API_KEY: &str = "AIzaSyDdP816MREB3SkjZO04QXbjsigfcI0GWOs";
pub(super) const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36";

/// The browser's `accounts.google.com/RotateCookiesPage` frame posts here to
/// exchange `__Secure-1PSIDTS`/`__Secure-3PSIDTS`; the reply carries them as
/// `Set-Cookie`. The body is the page's fixed placeholder, which is not JSON.
const ROTATE_URL: &str = "https://accounts.google.com/RotateCookies";
const ROTATE_BODY: &str = r#"[000,"-0000000000000000000"]"#;
/// Google answers 429 to rotations less than a minute apart. The browser
/// rotates roughly every ten minutes; a request-driven three hours keeps the
/// session fresh without that traffic.
const ROTATE_INTERVAL_MS: i64 = 3 * 60 * 60 * 1000;

/// Cookies Google replaces during a session's life. The others identify the
/// session and change only on a new login.
fn rotating(name: &str) -> bool {
    name.ends_with("SIDTS") || name.ends_with("SIDCC")
}

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
    /// The value each cookie held before its latest replacement. A request
    /// still in flight when a cookie rotates reports this value as sent, and
    /// it must not be saved back over the newer one.
    retired: BTreeMap<String, String>,
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
        let retired = secret
            .get("retired")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
            .collect();
        Ok(Self {
            cookies,
            auth_user,
            retired,
        })
    }

    fn replace(&mut self, name: &str, value: &str) {
        if self.cookies.get(name).is_some_and(|held| held == value) {
            return;
        }
        if let Some(old) = self.cookies.insert(name.to_owned(), value.to_owned()) {
            self.retired.insert(name.to_owned(), old);
        }
    }

    /// Take the session cookies of `Set-Cookie` headers Google returned.
    /// Deletions and cookies scoped outside google.com are ignored: a revoked
    /// session is reported by the RPC status, not by emptying the jar.
    pub fn issued<'a>(&mut self, headers: impl IntoIterator<Item = &'a HeaderValue>) {
        for header in headers {
            let Ok(text) = header.to_str() else { continue };
            let mut parts = text.split(';');
            let Some((name, value)) = parts.next().and_then(|pair| pair.trim().split_once('='))
            else {
                continue;
            };
            let (name, value) = (name.trim(), value.trim());
            if !session_cookie(name) || value.is_empty() || value == "EXPIRED" {
                continue;
            }
            let live = parts.all(|attribute| {
                let (key, value) = attribute
                    .split_once('=')
                    .map_or((attribute.trim(), ""), |(key, value)| {
                        (key.trim(), value.trim())
                    });
                if key.eq_ignore_ascii_case("domain") {
                    let domain = value.trim_start_matches('.').to_ascii_lowercase();
                    domain == "google.com" || domain.ends_with(".google.com")
                } else if key.eq_ignore_ascii_case("max-age") {
                    value.parse::<i64>().is_ok_and(|seconds| seconds > 0)
                } else if key.eq_ignore_ascii_case("expires") {
                    // Deletions are dated at the epoch or 1990.
                    value
                        .split(|c: char| !c.is_ascii_digit())
                        .find(|token| token.len() == 4)
                        .and_then(|year| year.parse::<u32>().ok())
                        .is_none_or(|year| year >= 2000)
                } else {
                    true
                }
            });
            if live {
                self.replace(name, value);
            }
        }
    }

    /// Take the rotating cookies of a Cookie header this channel sent, which
    /// the WAA signer may have refreshed while loading the page. A jar whose
    /// identity cookies differ belongs to another session and is left alone.
    fn sent(&mut self, header: &str) {
        let pairs = header
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .collect::<Vec<_>>();
        if pairs.iter().any(|(name, value)| {
            !rotating(name) && self.cookies.get(*name).is_some_and(|held| held != value)
        }) {
            return;
        }
        for (name, value) in pairs {
            if rotating(name)
                && session_cookie(name)
                && !value.is_empty()
                && self.retired.get(name).map(String::as_str) != Some(value)
            {
                self.replace(name, value);
            }
        }
    }

    /// `base` with this session's cookies in place of the imported ones.
    fn secret(&self, base: &Value) -> Result<Value, ChannelError> {
        let mut secret = base
            .as_object()
            .cloned()
            .ok_or(ChannelError::InvalidCredential)?;
        secret.remove("cookies");
        secret.insert("cookie".into(), self.cookie().into());
        secret.insert("auth_user".into(), self.auth_user.clone().into());
        if !self.retired.is_empty() {
            secret.insert("retired".into(), json!(self.retired));
        }
        Ok(secret.into())
    }

    /// Ask Google for fresh `PSIDTS` cookies. Best effort: whether the session
    /// is still good is decided by the MakerSuite call that follows.
    async fn rotate(&mut self, client: &dyn OutboundClient) -> Result<(), ChannelError> {
        if !self.cookies.contains_key("__Secure-1PSID")
            && !self.cookies.contains_key("__Secure-3PSID")
        {
            return Ok(());
        }
        let request = http::Request::builder()
            .method("POST")
            .uri(ROTATE_URL)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, "https://accounts.google.com")
            .header(header::USER_AGENT, USER_AGENT)
            .header(header::COOKIE, header_value(&self.cookie())?)
            .body(HttpBody::Bytes(ROTATE_BODY.into()))
            .map_err(|_| request::invalid("invalid RotateCookies request"))?;
        if let Ok(response) = client.send(request).await
            && response.status.is_success()
        {
            self.issued(response.headers.get_all(header::SET_COOKIE));
        }
        Ok(())
    }

    /// The account and the cookies that identify its session, without the ones
    /// Google rotates. Stable for as long as the login lasts.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn identity(&self) -> String {
        let mut identity = self.auth_user.clone();
        for (name, value) in &self.cookies {
            if !rotating(name) {
                identity.push_str(&format!("\n{name}={value}"));
            }
        }
        identity
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

/// Calls made by `refresh`, which reads their `Set-Cookie` and persists the
/// result itself.
fn refresh_call(uri: &http::Uri) -> bool {
    uri.path().ends_with("/RotateCookies") || uri.path().ends_with("/GetAiStudioBenefitTier")
}

impl CredentialRefresh for Playground {
    /// `expires_at_ms` is the next rotation time, not a token lifetime.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        now_ms: i64,
    ) -> Option<RefreshReason> {
        credential
            .expires_at_ms
            .is_some_and(|due| due <= now_ms)
            .then_some(RefreshReason::Scheduled)
    }

    /// Save cookies Google replaced outside a refresh: `Set-Cookie` on an RPC
    /// response, and rotating cookies the signer put in the request we sent.
    fn response_update(
        &self,
        credential: CredentialView<'_>,
        request: &http::Request<()>,
        headers: &mut HeaderMap,
    ) -> Option<CredentialUpdate> {
        if refresh_call(request.uri()) {
            return None;
        }
        let issued = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        headers.remove(header::SET_COOKIE);
        // Only Google calls carry the Cookie header; the signer's does not.
        let sent = request.headers().get(header::COOKIE)?.to_str().ok()?;
        // A due credential is about to be refreshed; writing a version under
        // that refresh would make it lose its compare-and-swap.
        if credential
            .expires_at_ms
            .is_some_and(|due| due <= now_ms().unwrap_or(i64::MAX))
        {
            return None;
        }
        let mut session = Session::parse(credential.secret).ok()?;
        let held = session.cookies.clone();
        session.sent(sent);
        session.issued(&issued);
        if session.cookies == held {
            return None;
        }
        Some(CredentialUpdate {
            secret: session.secret(credential.secret).ok()?,
            metadata: None,
            expires_at_ms: credential.expires_at_ms,
        })
    }

    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let mut session = Session::parse(context.credential.secret)?;
            session.rotate(context.client).await?;
            let response = context
                .client
                .send(request::post(
                    &config::endpoint(context.provider, "GetAiStudioBenefitTier", None),
                    session.headers(None)?,
                    &json!([]),
                )?)
                .await?;
            let status = response.status;
            session.issued(response.headers.get_all(header::SET_COOKIE));
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
            let secret = session.secret(context.credential.secret)?;
            let mut metadata = context
                .credential
                .metadata
                .as_object()
                .cloned()
                .unwrap_or_default();
            metadata.insert("plan".into(), plan.into());
            metadata.insert("benefit_tier".into(), header.into());
            Ok(CredentialUpdate {
                secret,
                metadata: Some(metadata.into()),
                expires_at_ms: Some(now_ms()? + ROTATE_INTERVAL_MS),
            })
        })
    }
}
